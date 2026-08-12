//! Async serial driver for Meshtastic devices.
//!
//! Meshtastic uses protobuf payloads over a SLIP-like serial stream. This
//! module implements only the small subset Archipelago needs: connect,
//! discover the local node, send/receive text packets, and provide synthetic
//! contacts to the existing mesh listener.

use super::protocol::{InboundFrame, ParsedContact};
use super::types::{DeviceInfo, DeviceType};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn};

const BAUD_RATE: u32 = 115200;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
const READ_BUF_SIZE: usize = 512;

const START1: u8 = 0x94;
const START2: u8 = 0xc3;
const TO_RADIO_MAX: usize = 512;
const BROADCAST_NUM: u32 = 0xffff_ffff;
const TEXT_MESSAGE_APP: u32 = 1;
const POSITION_APP: u32 = 3;
/// Meshtastic PortNum for NodeInfo (identity) packets — used to actively
/// advertise ourselves over the air so neighbours discover us, the parity
/// equivalent of meshcore's self-advert.
const NODEINFO_APP: u32 = 4;
/// Meshtastic PortNum for admin (config) packets.
const ADMIN_APP: u32 = 6;
/// AdminMessage.set_owner oneof field number (carries a `User`).
const ADMIN_SET_OWNER_FIELD: u64 = 32;
/// Meshtastic firmware caps long_name at ~40 bytes and short_name at 4 bytes.
const MESHTASTIC_LONG_NAME_MAX: usize = 39;
const MESHTASTIC_SHORT_NAME_MAX: usize = 4;
const STALE_RX_SECS: u32 = 24 * 60 * 60;
const PLAUSIBLE_RX_EPOCH_SECS: u32 = 1_700_000_000; // 2023-11-14

const TO_RADIO_PACKET: u64 = 1;
const TO_RADIO_WANT_CONFIG_ID: u64 = 3;
const TO_RADIO_HEARTBEAT: u64 = 7;

const FROM_RADIO_PACKET: u64 = 2;
const FROM_RADIO_MY_INFO: u64 = 3;
const FROM_RADIO_NODE_INFO: u64 = 4;
/// FromRadio.config (field 5): a `Config` block streamed during want_config.
const FROM_RADIO_CONFIG: u64 = 5;
const FROM_RADIO_CONFIG_COMPLETE_ID: u64 = 7;
const FROM_RADIO_REBOOTED: u64 = 8;
/// Upper bound for a single Meshtastic serial API protobuf frame. The serial
/// stream can contain firmware log text, so this is also used to reject false
/// 0x94c3 markers found inside logs instead of waiting forever for a bogus
/// length.
const FROM_RADIO_MAX: usize = 4096;

/// AdminMessage.set_config oneof field number (carries a `Config`). NB: 33 is
/// `set_channel` — `set_config` is 34 (verified against meshtastic/protobufs).
const ADMIN_SET_CONFIG_FIELD: u64 = 34;
/// AdminMessage.set_channel oneof field number (carries a `Channel`).
const ADMIN_SET_CHANNEL_FIELD: u64 = 33;
/// AdminMessage.reboot_seconds oneof field number (int32). Verified against
/// meshtastic/protobufs admin.proto: `reboot_seconds = 97` (NOT 40 — the
/// payload_variant numbers jump after the setters).
const ADMIN_REBOOT_SECONDS_FIELD: u64 = 97;
/// FromRadio.channel (field 10): a `Channel` streamed during want_config.
const FROM_RADIO_CHANNEL: u64 = 10;
const FROM_RADIO_QUEUE_STATUS: u64 = 11;
const FROM_RADIO_XMODEM_PACKET: u64 = 12;
const FROM_RADIO_METADATA: u64 = 13;
const FROM_RADIO_MQTT_CLIENT_PROXY_MESSAGE: u64 = 14;
const FROM_RADIO_FILE_INFO: u64 = 15;
const FROM_RADIO_CLIENT_NOTIFICATION: u64 = 16;
const FROM_RADIO_DEVICE_UI_CONFIG: u64 = 17;
const FROM_RADIO_LOCKDOWN_STATUS: u64 = 18;
const FROM_RADIO_REGION_PRESETS: u64 = 19;
/// Channel.role value for the PRIMARY channel (broadcasts ride here).
const CHANNEL_ROLE_PRIMARY: u64 = 1;
/// Channel.role value for a SECONDARY channel (extra channels we also decode).
const CHANNEL_ROLE_SECONDARY: u64 = 2;
/// Slot index our private archipelago channel occupies (secondary). Slot 0 is
/// kept as the off-the-shelf default public channel so archy interoperates with
/// stock Meshtastic devices (LongFast) AND picks up default-channel users.
const ARCHY_CHANNEL_INDEX: u64 = 1;
/// Meshtastic's default-channel PSK is the single byte 0x01 ("use the well-known
/// default key"); the firmware also reports it expanded to these 16 bytes. Treat
/// EITHER form as "the default channel" so we never reboot-loop re-setting it.
const DEFAULT_PSK_BYTE: &[u8] = &[1];
const DEFAULT_PSK_EXPANDED: &[u8] = &[
    0xd4, 0xf1, 0xbb, 0x3a, 0x20, 0x29, 0x07, 0x59, 0xf0, 0xbc, 0xff, 0xab, 0xcf, 0x4e, 0x69, 0x01,
];
/// Config.lora oneof field number (carries a `LoRaConfig`).
const CONFIG_LORA_FIELD: u64 = 6;
/// LoRaConfig field numbers we set when provisioning the radio's region.
const LORA_USE_PRESET_FIELD: u64 = 1;
/// LoRaConfig.modem_preset (field 2). Pinned to LONG_FAST (0) so every archy
/// radio computes the SAME over-the-air frequency/bandwidth. Omitting it (relying
/// on the firmware default) lets a radio keep a non-default preset persisted via
/// the phone app or a differing factory default — which puts radios on different
/// airwaves despite identical region + channel, so they silently never hear each
/// other. ModemPreset enum: LONG_FAST = 0.
const LORA_MODEM_PRESET_FIELD: u64 = 2;
const LORA_MODEM_PRESET_LONG_FAST: u64 = 0;
const LORA_REGION_FIELD: u64 = 7;
const LORA_HOP_LIMIT_FIELD: u64 = 8;
const LORA_TX_ENABLED_FIELD: u64 = 9;
/// RegionCode::UNSET — a radio in this state refuses to transmit or receive on
/// LoRa, so it can never mesh. Fresh-flashed radios ship UNSET.
const REGION_UNSET: u32 = 0;

/// Async Meshtastic device handle.
pub struct MeshtasticDevice {
    port: serial2_tokio::SerialPort,
    read_buf: Vec<u8>,
    node_num: Option<u32>,
    user_id: Option<String>,
    long_name: Option<String>,
    short_name: Option<String>,
    contacts: HashMap<u32, ParsedContact>,
    /// Real Curve25519 public keys, keyed by node-num, as learned from NodeInfo
    /// (`User.public_key`) or PKC-encrypted inbound packets (`MeshPacket
    /// .public_key`). Kept SEPARATE from `contacts[*].public_key_hex`, which is
    /// the synthetic node-num-derived routing key that `send_text_msg` relies
    /// on — we must not overwrite that or unicast routing breaks. This map only
    /// records which peers are PKC-capable, so we can tell a true end-to-end
    /// (PKI) DM from a channel-PSK fallback.
    peer_pubkeys: HashMap<u32, Vec<u8>>,
    /// The radio's currently-configured LoRa region code, learned from the
    /// `Config.lora` block during `initialize`. `None` until that frame is
    /// seen; `Some(REGION_UNSET)` for a fresh radio that has never had a region
    /// set (which means it is RF-silent). Used to decide whether we need to
    /// provision the operator-configured region — and to avoid a reboot loop by
    /// only writing when it actually differs.
    current_region: Option<u32>,
    /// The radio's currently-configured LoRa modem preset, learned alongside
    /// `current_region` from the same `Config.lora` block. Unlike region
    /// (deliberately never overridden once set), this SHOULD be authoritative
    /// -- two archy radios with mismatched presets (different spreading
    /// factor/bandwidth) can't decode each other even on the correct region.
    /// `None` until a Config frame is seen -- never write speculatively before
    /// we've actually observed the radio's real value.
    current_modem_preset: Option<u32>,
    /// The radio's current PRIMARY channel as `(name, psk)`, learned from the
    /// `Channel` blocks during `initialize`. Two radios only decode each other
    /// when their primary channel (name + psk → channel hash) matches, so archy
    /// provisions a shared channel here the same way it provisions the region.
    /// `None` until a primary `Channel` frame is seen.
    current_primary_channel: Option<(String, Vec<u8>)>,
    /// The radio's current SECONDARY channel at `ARCHY_CHANNEL_INDEX`, learned
    /// from `want_config`. This is where our private "archipelago" channel lives
    /// (slot 0 stays the public default). `None` until that slot's `Channel`
    /// frame is seen.
    current_secondary_channel: Option<(String, Vec<u8>)>,
    device_path: String,
    /// PKI-encryption status of the most recent inbound text frame yielded by
    /// `try_recv_frame`. The synthetic meshcore-style frame can't carry it, so
    /// the session loop reads it via `take_rx_encrypted()` right after dispatch
    /// to stamp the message's E2E pill. Set true only for `pki_encrypted` DMs.
    last_rx_encrypted: bool,
    /// Set when the radio announces it just rebooted (`FromRadio.rebooted`). A
    /// rebooted firmware drops every client's `want_config` session, so it stops
    /// streaming RECEIVED packets to us (we keep getting only our own
    /// queue-status). We must re-send `want_config` to re-subscribe to the live
    /// packet stream — otherwise inbound messages silently never surface after
    /// any config write (region/channel/owner all reboot the radio). Consumed in
    /// `try_recv_frame`, which re-issues the handshake.
    pending_reinit: bool,
}

/// Read-only snapshot of the radio config learned during `initialize`'s
/// `want_config` stream — surfaced by the hot-swap probe so the setup modal
/// can show what's currently on a just-plugged radio.
#[derive(Debug, Clone)]
pub struct MeshtasticProbedConfig {
    pub region: Option<String>,
    pub modem_preset: Option<String>,
    pub primary_channel: Option<String>,
    pub secondary_channel: Option<String>,
}

impl MeshtasticDevice {
    /// See [`MeshtasticProbedConfig`]. Everything here was learned via reads
    /// (`want_config`) — safe for a probe that must not modify the device.
    pub fn probed_config(&self) -> MeshtasticProbedConfig {
        MeshtasticProbedConfig {
            region: self
                .current_region
                .and_then(region_code_to_name)
                .map(str::to_string),
            modem_preset: self
                .current_modem_preset
                .and_then(modem_preset_name)
                .map(str::to_string),
            primary_channel: self.current_primary_channel.as_ref().map(|(name, _)| {
                if name.is_empty() {
                    "(default public)".to_string()
                } else {
                    name.clone()
                }
            }),
            secondary_channel: self
                .current_secondary_channel
                .as_ref()
                .map(|(name, _)| name.clone()),
        }
    }
    pub async fn open(path: &str) -> Result<Self> {
        match tokio::fs::metadata(path).await {
            Ok(meta) => {
                debug!(path = %path, permissions = ?meta.permissions(), "Device node exists")
            }
            Err(e) => anyhow::bail!("Serial device {} not accessible: {}", path, e),
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
        info!(path = %path, baud = BAUD_RATE, "Opened Meshtastic serial port");

        Ok(Self {
            port,
            read_buf: Vec::with_capacity(READ_BUF_SIZE),
            node_num: None,
            user_id: None,
            long_name: None,
            short_name: None,
            contacts: HashMap::new(),
            peer_pubkeys: HashMap::new(),
            current_region: None,
            current_modem_preset: None,
            current_primary_channel: None,
            current_secondary_channel: None,
            device_path: path.to_string(),
            last_rx_encrypted: false,
            pending_reinit: false,
        })
    }

    pub async fn initialize(&mut self) -> Result<DeviceInfo> {
        info!(path = %self.device_path, "Starting Meshtastic handshake");
        self.send_to_radio(&encode_want_config()).await?;

        let deadline = tokio::time::Instant::now() + READ_TIMEOUT;
        let mut saw_meshtastic_frame = false;
        let mut saw_config_complete = false;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(
                remaining.min(Duration::from_millis(250)),
                self.read_from_radio(),
            )
            .await
            {
                Ok(Ok(Some(frame))) => {
                    saw_meshtastic_frame = true;
                    if matches!(
                        decode_top_level_variant(&frame),
                        Some((FROM_RADIO_CONFIG_COMPLETE_ID, _))
                    ) {
                        saw_config_complete = true;
                    }
                    self.handle_from_radio(&frame);
                    if saw_config_complete && self.node_num.is_some() {
                        break;
                    }
                }
                Ok(Ok(None)) | Err(_) => {}
                Ok(Err(e)) => return Err(e),
            }
        }

        if !saw_meshtastic_frame {
            anyhow::bail!("No Meshtastic serial API response");
        }

        let node_id = self
            .node_num
            .ok_or_else(|| anyhow::anyhow!("Meshtastic serial API did not provide MyInfo"))?;
        if self.user_id.is_none() && self.long_name.is_none() && self.short_name.is_none() {
            anyhow::bail!("Meshtastic serial API did not provide node identity");
        }
        let firmware_version = self
            .long_name
            .clone()
            .or_else(|| self.user_id.clone())
            .unwrap_or_else(|| "Meshtastic".to_string());

        info!(node_id, name = %firmware_version, "Meshtastic identity");
        Ok(DeviceInfo {
            firmware_version,
            node_id,
            max_contacts: 200,
            device_type: DeviceType::Meshtastic,
        })
    }

    /// Rename the connected Meshtastic radio to match the node's server name so
    /// it's findable from external Meshtastic apps (phone/desktop) on the same
    /// mesh. Previously this only updated the in-memory field and never told the
    /// device — so the radio kept its firmware-default name ("Meshtastic xxxx").
    ///
    /// We push an `AdminMessage { set_owner: User { long_name, short_name } }` to
    /// the locally-connected node (an admin packet addressed to our own
    /// `node_num`, on the ADMIN_APP port). Local admin over the serial link needs
    /// no session passkey, so this is the same path the official phone/CLI client
    /// uses for "set owner".
    pub async fn set_advert_name(&mut self, name: &str) -> Result<()> {
        let long_name: String = name.chars().take(MESHTASTIC_LONG_NAME_MAX).collect();
        let short_name = derive_short_name(name).unwrap_or_else(|| {
            self.short_name
                .clone()
                .unwrap_or_else(|| "NODE".to_string())
        });

        let Some(node_num) = self.node_num else {
            // No local node number yet (initialize() not completed) — can't
            // address a local admin packet. Record the intent so advert_name()
            // still reflects it, but skip the device write.
            warn!("Meshtastic set_advert_name: node_num unknown, skipping device write");
            self.long_name = Some(long_name);
            self.short_name = Some(short_name);
            return Ok(());
        };

        // User { id?(1), long_name(2), short_name(3) }. Echo back the existing id
        // when known so the firmware keeps the node's stable `!xxxxxxxx` id.
        let mut user = Vec::new();
        if let Some(id) = &self.user_id {
            encode_len_field(1, id.as_bytes(), &mut user);
        }
        encode_len_field(2, long_name.as_bytes(), &mut user);
        encode_len_field(3, short_name.as_bytes(), &mut user);

        // AdminMessage { set_owner(32): User }
        let mut admin = Vec::new();
        encode_len_field(ADMIN_SET_OWNER_FIELD, &user, &mut admin);

        // Admin packet to ourselves on the ADMIN_APP port.
        let packet = encode_mesh_packet(node_num, ADMIN_APP, &admin);
        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
            .context("Failed to send Meshtastic set_owner admin packet")?;

        info!(node_num, long_name = %long_name, short_name = %short_name, "Set Meshtastic device owner");
        self.long_name = Some(long_name);
        self.short_name = Some(short_name);
        Ok(())
    }

    /// Ensure the radio is provisioned for the operator-configured LoRa region.
    /// A freshly-flashed Meshtastic radio ships with `region = UNSET`, which
    /// makes the firmware refuse to transmit or receive anything — so two such
    /// radios can never see each other and the mesh appears empty. This is the
    /// Meshtastic analog of how a meshcore radio comes up on its configured
    /// band: archy brings every node onto the same region automatically.
    ///
    /// Returns `Ok(true)` when it actually wrote a new region (the device then
    /// reboots to apply it, so the caller should restart the session). Returns
    /// `Ok(false)` when no change was needed (already correct, no region
    /// configured, or an unrecognised region string) — never reboot-loops.
    pub async fn ensure_lora_region(&mut self, region: Option<&str>) -> Result<bool> {
        let Some(region_str) = region else {
            return Ok(false);
        };
        let Some(code) = region_name_to_code(region_str) else {
            warn!(
                region = region_str,
                "Unknown LoRa region in mesh-config — leaving radio region unchanged"
            );
            return Ok(false);
        };
        if code == REGION_UNSET {
            // Operator explicitly asked for UNSET (or blank) — don't fight it.
            return Ok(false);
        }
        match self.current_region {
            // The radio already has a REAL region (US, EU_868, ANZ, …). RESPECT
            // it — never override the region the user flashed/configured. Forcing
            // our configured region onto, say, a US radio would put it on an
            // illegal band and cut it off from its local mesh. Off-the-shelf
            // devices keep whatever region they came with; `code` (the
            // mesh-config region) is only the fallback for a fresh radio below.
            Some(cur) if cur != REGION_UNSET => {
                if cur != code {
                    debug!(
                        device_region = cur,
                        configured_region = code,
                        "Respecting the radio's own LoRa region (not overriding with the configured one)"
                    );
                }
                // Region is untouched either way, but the modem preset IS
                // authoritative (backlog #12): two archy radios on mismatched
                // presets can't decode each other even on the same region.
                // Only acts once we've actually observed a real (non-None)
                // preset that's wrong — never speculative, so this can't
                // reboot-loop (the write sets LONG_FAST, the next Config
                // frame confirms it, and this branch goes quiet).
                match self.current_modem_preset {
                    Some(p) if p != LORA_MODEM_PRESET_LONG_FAST as u32 => {
                        debug!(
                            device_preset = p,
                            "Modem preset drifted from LONG_FAST — re-provisioning (region unchanged)"
                        );
                        self.set_lora_region(cur).await?;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            // Region is UNSET → a fresh radio is RF-silent and can't mesh at all.
            // Set the operator-configured region so it can transmit/receive.
            Some(_) => {
                self.set_lora_region(code).await?;
                Ok(true)
            }
            // Region unknown (never reported in want_config) — don't guess /
            // don't override; leave it for the user to set.
            None => Ok(false),
        }
    }

    /// Write a LoRa region to the locally-connected radio via an
    /// `AdminMessage { set_config: Config { lora: LoRaConfig { … } } }` on the
    /// ADMIN_APP port — the same local-admin path `set_advert_name` uses (no
    /// session passkey needed over serial). We send a minimal, valid preset
    /// config: `use_preset` + `LONG_FAST` (the default modem preset), the
    /// chosen `region`, a sane `hop_limit`, and `tx_enabled`. The firmware
    /// reboots to apply the change.
    pub async fn set_lora_region(&mut self, region_code: u32) -> Result<()> {
        let Some(node_num) = self.node_num else {
            anyhow::bail!("Meshtastic set_lora_region: node_num unknown");
        };

        // LoRaConfig { use_preset(1)=true, modem_preset(2)=LONG_FAST, region(7)=code,
        // hop_limit(8)=3, tx_enabled(9)=true }. We pin modem_preset explicitly
        // (rather than relying on the firmware default) so every archy radio lands
        // on the SAME frequency/bandwidth — otherwise a radio carrying a stale
        // non-default preset stays on different airwaves and silently never meshes.
        // tx_power defaults to max, which is what we want for a stock mesh.
        let mut lora = Vec::new();
        encode_varint_field_into(LORA_USE_PRESET_FIELD, 1, &mut lora);
        encode_varint_field_into(
            LORA_MODEM_PRESET_FIELD,
            LORA_MODEM_PRESET_LONG_FAST,
            &mut lora,
        );
        encode_varint_field_into(LORA_REGION_FIELD, region_code as u64, &mut lora);
        encode_varint_field_into(LORA_HOP_LIMIT_FIELD, 3, &mut lora);
        encode_varint_field_into(LORA_TX_ENABLED_FIELD, 1, &mut lora);

        // Config { lora(6): LoRaConfig }
        let mut config = Vec::new();
        encode_len_field(CONFIG_LORA_FIELD, &lora, &mut config);

        // AdminMessage { set_config(33): Config }
        let mut admin = Vec::new();
        encode_len_field(ADMIN_SET_CONFIG_FIELD, &config, &mut admin);

        let packet = encode_mesh_packet(node_num, ADMIN_APP, &admin);
        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
            .context("Failed to send Meshtastic set_config(LoRa region) admin packet")?;

        info!(
            node_num,
            region_code, "Set Meshtastic LoRa region (device will reboot to apply)"
        );
        self.current_region = Some(region_code);
        Ok(())
    }

    /// Reboot the locally-connected radio via `AdminMessage { reboot_seconds }`
    /// on the ADMIN_APP port — the same local-admin path `set_advert_name` /
    /// `set_lora_region` use (no session passkey needed over serial). The
    /// firmware reboots after `seconds`, which clears a wedged / RX-deaf radio
    /// (a radio that has stopped hearing the mesh while still transmitting) and
    /// re-runs its LoRa init. The listener's reboot→reconnect loop reopens the
    /// serial link when it comes back.
    pub async fn reboot(&mut self, seconds: i64) -> Result<()> {
        let Some(node_num) = self.node_num else {
            anyhow::bail!("Meshtastic reboot: node_num unknown");
        };
        // AdminMessage { reboot_seconds(97): int32 }. We only ever pass a small
        // positive delay, which encodes as a plain varint.
        let mut admin = Vec::new();
        encode_varint_field_into(ADMIN_REBOOT_SECONDS_FIELD, seconds as u64, &mut admin);

        let packet = encode_mesh_packet(node_num, ADMIN_APP, &admin);
        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
            .context("Failed to send Meshtastic reboot admin packet")?;

        info!(
            node_num,
            seconds, "Sent Meshtastic radio reboot (device will reboot to recover)"
        );
        Ok(())
    }

    /// Provision archy's two channels so the radio works like off-the-shelf
    /// Meshtastic AND carries our private group:
    ///   - slot 0 (PRIMARY)  = the DEFAULT public channel (name "", default key)
    ///     → archy interoperates with every stock device on LongFast and picks
    ///       up default-channel users; our own NodeInfo broadcasts ride here.
    ///   - slot 1 (SECONDARY) = "archipelago" (deterministic psk from the name)
    ///     → the private archy↔archy group channel (parity with meshcore).
    ///
    /// Writes at most ONE channel per call (each write reboots the radio), so the
    /// existing reboot→reconnect→re-check loop converges over a couple of cycles
    /// without ever reboot-looping. Returns `Ok(true)` when it wrote something.
    pub async fn ensure_channel(&mut self, channel_name: Option<&str>) -> Result<bool> {
        // 1) Primary must be the default public channel (off-the-shelf interop).
        if !primary_is_default(&self.current_primary_channel) {
            self.set_channel(0, "", DEFAULT_PSK_BYTE, CHANNEL_ROLE_PRIMARY)
                .await?;
            return Ok(true);
        }
        // 2) Secondary slot = our private archipelago channel (when configured).
        let Some(channel_name) = channel_name.filter(|n| !n.is_empty()) else {
            return Ok(false);
        };
        let desired_psk = derive_channel_psk(channel_name);
        let already = matches!(
            &self.current_secondary_channel,
            Some((name, psk)) if name == channel_name && psk == &desired_psk
        );
        if already {
            Ok(false)
        } else {
            self.set_channel(
                ARCHY_CHANNEL_INDEX,
                channel_name,
                &desired_psk,
                CHANNEL_ROLE_SECONDARY,
            )
            .await?;
            Ok(true)
        }
    }

    /// Write a channel slot via `AdminMessage { set_channel: Channel { … } }`
    /// (the same local-admin path as `set_advert_name`). The firmware reboots to
    /// apply it.
    pub async fn set_channel(
        &mut self,
        index: u64,
        name: &str,
        psk: &[u8],
        role: u64,
    ) -> Result<()> {
        let Some(node_num) = self.node_num else {
            anyhow::bail!("Meshtastic set_channel: node_num unknown");
        };

        // ChannelSettings { psk(2), name(3) }
        let mut settings = Vec::new();
        encode_len_field(2, psk, &mut settings);
        encode_len_field(3, name.as_bytes(), &mut settings);

        // Channel { index(1), settings(2), role(3) }
        let mut channel = Vec::new();
        encode_varint_field_into(1, index, &mut channel);
        encode_len_field(2, &settings, &mut channel);
        encode_varint_field_into(3, role, &mut channel);

        // AdminMessage { set_channel(33): Channel }
        let mut admin = Vec::new();
        encode_len_field(ADMIN_SET_CHANNEL_FIELD, &channel, &mut admin);

        let packet = encode_mesh_packet(node_num, ADMIN_APP, &admin);
        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
            .context("Failed to send Meshtastic set_channel admin packet")?;

        let slot = if role == CHANNEL_ROLE_PRIMARY {
            "primary(default)"
        } else {
            "secondary(archipelago)"
        };
        info!(node_num, index, channel = %name, slot, "Set Meshtastic channel (device will reboot to apply)");
        if role == CHANNEL_ROLE_PRIMARY {
            self.current_primary_channel = Some((name.to_string(), psk.to_vec()));
        } else {
            self.current_secondary_channel = Some((name.to_string(), psk.to_vec()));
        }
        Ok(())
    }

    pub async fn send_self_advert(&mut self) -> Result<()> {
        self.send_to_radio(&encode_heartbeat()).await?;
        self.send_time_broadcast().await
    }

    /// Lightweight serial keepalive: a bare `ToRadio.heartbeat`. The firmware's
    /// PhoneAPI treats a client that goes quiet as gone and can stop streaming
    /// received packets to it; a once-a-minute advert heartbeat is too sparse, so
    /// the session loop pings this every few seconds to keep the inbound stream
    /// flowing. No NodeInfo/Position side effects, so it's cheap to call often.
    pub async fn send_keepalive(&mut self) -> Result<()> {
        self.send_to_radio(&encode_heartbeat()).await
    }

    /// Broadcast a minimal Position payload carrying current epoch time. The
    /// Meshtastic protobuf explicitly documents `Position.time` as the path for
    /// phone/API clients to set time on mesh devices without GPS/RTC. This keeps
    /// stock Meshtastic clients from rendering incoming Archipelago-originated
    /// packets as Jan 1 1970 when their radio clock is unset.
    pub async fn send_time_broadcast(&mut self) -> Result<()> {
        let now = now_unix_secs();
        let mut position = Vec::new();
        encode_fixed32_field(4, now, &mut position);
        encode_fixed32_field(7, now, &mut position);
        let packet = encode_mesh_packet(BROADCAST_NUM, POSITION_APP, &position);
        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
    }

    /// Build our own `User` protobuf (id/long_name/short_name) for a NodeInfo
    /// advert. Returns `None` until the handshake has learned our identity.
    fn build_self_user(&self) -> Option<Vec<u8>> {
        let mut user = Vec::new();
        if let Some(id) = &self.user_id {
            encode_len_field(1, id.as_bytes(), &mut user);
        }
        if let Some(long_name) = &self.long_name {
            encode_len_field(2, long_name.as_bytes(), &mut user);
        }
        if let Some(short_name) = &self.short_name {
            encode_len_field(3, short_name.as_bytes(), &mut user);
        }
        if user.is_empty() {
            None
        } else {
            Some(user)
        }
    }

    /// Actively advertise our identity over the air by broadcasting a NodeInfo
    /// packet (our `User`) on the primary channel. Meshtastic radios otherwise
    /// only emit NodeInfo on boot and every few hours, so without this two
    /// already-running nodes can sit forever without discovering each other.
    /// This is the Meshtastic analog of meshcore's periodic self-advert.
    ///
    /// `want_response` solicits each neighbour to reply with its own NodeInfo —
    /// use it on connect for immediate two-way discovery; leave it off for the
    /// periodic beacon so a busy mesh doesn't trigger reply storms.
    pub async fn send_nodeinfo_broadcast(&mut self, want_response: bool) -> Result<()> {
        let Some(user) = self.build_self_user() else {
            debug!("Meshtastic NodeInfo advert skipped — local identity not known yet");
            return Ok(());
        };

        // Data { portnum(1)=NODEINFO_APP, payload(2)=User, want_response(3)? }
        let mut data = Vec::new();
        encode_varint_field_into(1, NODEINFO_APP as u64, &mut data);
        encode_len_field(2, &user, &mut data);
        if want_response {
            encode_varint_field_into(3, 1, &mut data);
        }

        // MeshPacket { to(2)=BROADCAST (fixed32), decoded(4)=Data }. The firmware
        // fills in `from` = our node-num when it transmits.
        let mut packet = Vec::new();
        encode_fixed32_field(2, BROADCAST_NUM, &mut packet);
        encode_len_field(4, &data, &mut packet);

        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
            .context("Failed to send Meshtastic NodeInfo broadcast")?;
        debug!(want_response, "Broadcast Meshtastic NodeInfo advert");
        Ok(())
    }

    pub async fn send_channel_text(&mut self, channel: u8, msg: &[u8]) -> Result<()> {
        let text = String::from_utf8_lossy(msg);
        let mut packet = encode_mesh_packet(BROADCAST_NUM, TEXT_MESSAGE_APP, text.as_bytes());
        // MeshPacket.channel (field 3, varint) selects which channel slot the
        // firmware encrypts and transmits on. 0 = primary (the default public
        // LongFast channel); a non-zero slot is our secondary archipelago
        // channel. Appended after encode_mesh_packet's fields — protobuf fields
        // are order-independent.
        if channel != 0 {
            encode_varint_field_into(3, channel as u64, &mut packet);
        }
        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
    }

    /// Native Meshtastic unicast DM. Our synthetic Meshtastic pubkeys carry the
    /// numeric node-id in their first 4 bytes (little-endian, see
    /// `synthetic_pubkey`), so `dest_pubkey_prefix` directly yields the
    /// destination node number. We send a directed MeshPacket (`to` = node num)
    /// rather than a `BROADCAST_NUM` channel blast — this is the Meshtastic
    /// analog of the meshcore `CMD_SEND_TXT_MSG` fix: the message is delivered
    /// as a real DM (only the recipient's client surfaces it) instead of
    /// polluting the shared primary channel where every node would see it.
    ///
    /// If the prefix decodes to node 0 / broadcast (e.g. a non-Meshtastic
    /// synthetic key routed here by mistake), fall back to a channel send so the
    /// device interface stays uniform and the message still goes out.
    pub async fn send_text_msg(&mut self, dest_pubkey_prefix: &[u8; 6], msg: &[u8]) -> Result<()> {
        let node_num = u32::from_le_bytes([
            dest_pubkey_prefix[0],
            dest_pubkey_prefix[1],
            dest_pubkey_prefix[2],
            dest_pubkey_prefix[3],
        ]);
        if node_num == 0 || node_num == BROADCAST_NUM {
            return self.send_channel_text(0, msg).await;
        }
        let text = String::from_utf8_lossy(msg);
        let packet = encode_mesh_packet(node_num, TEXT_MESSAGE_APP, text.as_bytes());
        self.send_to_radio(&encode_to_radio_variant(TO_RADIO_PACKET, &packet))
            .await
    }

    /// Meshtastic has no meshcore-style contact table; these are no-ops so the
    /// device interface stays uniform.
    pub async fn remove_contact(&mut self, _pubkey: &[u8; 32]) -> Result<()> {
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
        Ok(())
    }

    pub async fn get_contacts(&mut self) -> Result<Vec<ParsedContact>> {
        if self.contacts.is_empty() {
            self.send_to_radio(&encode_want_config()).await?;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            while tokio::time::Instant::now() < deadline {
                match self.read_from_radio().await? {
                    Some(frame) => {
                        let config_complete = matches!(
                            decode_top_level_variant(&frame),
                            Some((FROM_RADIO_CONFIG_COMPLETE_ID, _))
                        );
                        self.handle_from_radio(&frame);
                        if config_complete || !self.contacts.is_empty() {
                            break;
                        }
                    }
                    None => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        }
        // Stamp E2E capability per contact from `peer_pubkeys` (the real
        // Curve25519 keys learned from NodeInfo / inbound PKC packets), keyed by
        // node-num — which is exactly the contacts map key. This lets the send
        // path mark a Sent DM to ANY PKC-capable peer (e.g. a stock device that
        // shared its key) as E2E, not just archipelago peers.
        Ok(self
            .contacts
            .iter()
            .map(|(num, c)| {
                let mut c = c.clone();
                c.pkc_capable = self.peer_is_pkc_capable(*num);
                c
            })
            .collect())
    }

    pub async fn reset_contact_path(&mut self, _pubkey: &[u8; 32]) -> Result<()> {
        Ok(())
    }

    pub async fn sync_messages(&mut self) -> Result<Vec<InboundFrame>> {
        Ok(Vec::new())
    }

    pub async fn try_recv_frame(&mut self) -> Result<Option<InboundFrame>> {
        // Drain a bounded batch of frames per poll, processing EACH for its side
        // effects (my_info/config/channel/node_info) and returning the first that
        // yields an inbound text frame. The old one-frame-per-poll behavior
        // returned Ok(None) for every non-text frame, so the caller slept 50ms
        // between frames; under Meshtastic's frequent NodeInfo/telemetry stream a
        // received text packet queued behind them and the read buffer's 64KB cap
        // could drain (drop) it before it was ever decoded — silently killing
        // reception while sends kept working. Draining keeps the buffer short so
        // the text frame is decoded the same poll it arrives. Bounded to 64 so a
        // continuous flood still yields back to the session select! loop.
        for _ in 0..64 {
            let Some(frame) = self.read_from_radio().await? else {
                break;
            };
            let inbound = self.handle_from_radio(&frame);
            // If the radio announced a reboot while draining, re-subscribe to the
            // live packet stream BEFORE returning, so we don't go deaf to inbound
            // packets for the rest of the session. (A reboot drops our want_config
            // session on the firmware side.)
            if self.pending_reinit {
                self.pending_reinit = false;
                if let Err(e) = self.send_to_radio(&encode_want_config()).await {
                    warn!("Failed to re-request config after radio reboot: {}", e);
                } else {
                    info!(
                        "Re-requested Meshtastic config after reboot — packet stream resubscribed"
                    );
                }
            }
            if let Some(inbound) = inbound {
                return Ok(Some(inbound));
            }
        }
        Ok(None)
    }

    /// Whether we've learned `node_num`'s real PKI (Curve25519) key — from a
    /// NodeInfo `public_key` or an inbound PKC DM — meaning the firmware can
    /// deliver DMs to/from it end-to-end encrypted instead of falling back to
    /// the channel PSK. Consumed by `get_contacts` to stamp `ParsedContact
    /// .pkc_capable`, so the send path can mark a Sent DM to any PKC-capable
    /// peer as E2E (not just archipelago peers).
    pub fn peer_is_pkc_capable(&self, node_num: u32) -> bool {
        self.peer_pubkeys
            .get(&node_num)
            .is_some_and(|k| !k.is_empty())
    }

    pub fn advert_name(&self) -> Option<String> {
        self.long_name
            .clone()
            .or_else(|| self.short_name.clone())
            .or_else(|| self.user_id.clone())
    }

    async fn send_to_radio(&mut self, payload: &[u8]) -> Result<()> {
        if payload.len() > TO_RADIO_MAX {
            anyhow::bail!("Meshtastic payload too large: {} bytes", payload.len());
        }
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.push(START1);
        frame.push(START2);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        frame.extend_from_slice(payload);
        tokio::time::timeout(WRITE_TIMEOUT, self.port.write_all(&frame))
            .await
            .context("Meshtastic serial write timed out")?
            .context("Meshtastic serial write failed")?;
        Ok(())
    }

    async fn read_from_radio(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(frame) = decode_serial_frame(&mut self.read_buf) {
            return Ok(Some(frame));
        }

        // Drain aggressively. Meshtastic firmware interleaves verbose debug-log
        // text with protobuf frames on the same serial line, so a single small
        // read per poll can fall behind the byte stream, overflow the OS serial
        // buffer, and corrupt/drop inbound frames — which silently kills message
        // reception while leaving sends working. Pull up to a bounded burst of
        // bytes per call, decoding as soon as a complete frame appears.
        let mut tmp = [0u8; READ_BUF_SIZE];
        for _ in 0..32 {
            match tokio::time::timeout(Duration::from_millis(30), self.port.read(&mut tmp)).await {
                Ok(Ok(0)) => anyhow::bail!("Meshtastic serial port closed"),
                Ok(Ok(n)) => {
                    self.read_buf.extend_from_slice(&tmp[..n]);
                    if let Some(frame) = decode_serial_frame(&mut self.read_buf) {
                        return Ok(Some(frame));
                    }
                    // Bound memory if it's a pure-debug flood with no frames:
                    // keep only from the last possible frame-start marker.
                    if self.read_buf.len() > 64 * 1024 {
                        if let Some(pos) = self
                            .read_buf
                            .windows(2)
                            .rposition(|w| w == [START1, START2])
                        {
                            self.read_buf.drain(..pos);
                        } else {
                            self.read_buf.clear();
                        }
                    }
                }
                Ok(Err(e)) => return Err(e).context("Meshtastic serial read error"),
                Err(_) => break, // no more bytes available right now
            }
        }

        Ok(decode_serial_frame(&mut self.read_buf))
    }

    fn handle_from_radio(&mut self, frame: &[u8]) -> Option<InboundFrame> {
        let Some((field, value)) = decode_top_level_variant(frame) else {
            debug!(
                len = frame.len(),
                head = %hex::encode(&frame[..frame.len().min(8)]),
                "Meshtastic FromRadio frame did not decode to a known top-level field"
            );
            return None;
        };
        debug!(field, value_len = value.len(), "Meshtastic FromRadio field");
        match field {
            FROM_RADIO_MY_INFO => {
                if let Some((node_num, user_id)) = parse_my_info(value) {
                    self.node_num = Some(node_num);
                    if let Some(user_id) = user_id {
                        self.user_id = Some(user_id);
                    }
                }
                None
            }
            FROM_RADIO_NODE_INFO => {
                self.update_node_info(value);
                None
            }
            FROM_RADIO_PACKET => self.packet_to_inbound_frame(value),
            FROM_RADIO_CONFIG => {
                // Only the LoRa sub-config carries a region; other Config
                // variants (device/position/…) return None and are ignored.
                if let Some((region, modem_preset)) = parse_config_lora_region(value) {
                    self.current_region = Some(region);
                    self.current_modem_preset = Some(modem_preset);
                    debug!(
                        region,
                        modem_preset, "Meshtastic LoRa region/preset from device config"
                    );
                }
                None
            }
            FROM_RADIO_CHANNEL => {
                if let Some((index, role, name, psk)) = parse_channel(value) {
                    debug!(index, role, name = %name, psk_len = psk.len(), "Meshtastic channel from device");
                    if role == CHANNEL_ROLE_PRIMARY {
                        self.current_primary_channel = Some((name, psk));
                    } else if index == ARCHY_CHANNEL_INDEX {
                        self.current_secondary_channel = Some((name, psk));
                    }
                }
                None
            }
            FROM_RADIO_REBOOTED => {
                // The radio just rebooted (a config write, or a manual/OTA
                // reboot). Its firmware has dropped our `want_config` session,
                // so it will no longer stream RECEIVED packets to us — we'd be
                // left hearing only our own queue-status and silently miss every
                // inbound message. Flag a re-subscribe; `try_recv_frame` re-issues
                // `want_config` to resume the live packet stream.
                warn!("Meshtastic radio rebooted — will re-request config to resume packet stream");
                self.pending_reinit = true;
                None
            }
            FROM_RADIO_CONFIG_COMPLETE_ID
            | FROM_RADIO_QUEUE_STATUS
            | FROM_RADIO_XMODEM_PACKET
            | FROM_RADIO_METADATA
            | FROM_RADIO_MQTT_CLIENT_PROXY_MESSAGE
            | FROM_RADIO_FILE_INFO
            | FROM_RADIO_CLIENT_NOTIFICATION
            | FROM_RADIO_DEVICE_UI_CONFIG
            | FROM_RADIO_LOCKDOWN_STATUS
            | FROM_RADIO_REGION_PRESETS => None,
            other => {
                debug!(
                    field = other,
                    len = value.len(),
                    "Unhandled Meshtastic FromRadio field"
                );
                None
            }
        }
    }

    fn update_node_info(&mut self, data: &[u8]) {
        if let Some(node) = parse_node_info(data) {
            if let Some(pk) = node.public_key.as_ref() {
                if self.peer_pubkeys.insert(node.num, pk.clone()).is_none() {
                    debug!(
                        node = node.num,
                        key_len = pk.len(),
                        "Meshtastic peer is PKC-capable (NodeInfo public_key)"
                    );
                }
            }
            let key = synthetic_pubkey(node.num);
            let name = node
                .long_name
                .or(node.short_name)
                .or(node.id)
                .unwrap_or_else(|| format!("Meshtastic !{:08x}", node.num));
            if Some(node.num) == self.node_num {
                self.long_name = Some(name.clone());
            }
            // Preserve any signal-quality/position readings already
            // accumulated for this node (packet_to_inbound_frame's
            // `contacts.entry(...).or_insert_with` path) — a NodeInfo update
            // shouldn't wipe them back to None just because it replaces the
            // identity fields.
            let (rssi, snr, lat, lon) = self
                .contacts
                .get(&node.num)
                .map(|c| (c.rssi, c.snr, c.lat, c.lon))
                .unwrap_or_default();
            self.contacts.insert(
                node.num,
                ParsedContact {
                    public_key_hex: hex::encode(key),
                    advert_name: name,
                    last_advert: node.last_heard.unwrap_or_default(),
                    contact_type: 1,
                    path_len: 0xff,
                    flags: 0,
                    // Stamped fresh from `peer_pubkeys` in `get_contacts`.
                    pkc_capable: false,
                    rssi,
                    snr,
                    lat,
                    lon,
                    arch_pubkey_hex: None,
                },
            );
        }
    }

    fn packet_to_inbound_frame(&mut self, data: &[u8]) -> Option<InboundFrame> {
        packet_to_inbound_frame(
            data,
            self.node_num,
            &mut self.contacts,
            &mut self.peer_pubkeys,
        )
    }

    /// Take + clear the PKI-E2E status of the last inbound text frame. The
    /// session loop calls this right after dispatching a received frame to stamp
    /// the message's E2E pill (meshtastic DMs are E2E only when PKI-encrypted).
    pub fn take_rx_encrypted(&mut self) -> bool {
        std::mem::take(&mut self.last_rx_encrypted)
    }
}

fn packet_to_inbound_frame(
    data: &[u8],
    local_node_num: Option<u32>,
    contacts: &mut HashMap<u32, ParsedContact>,
    peer_pubkeys: &mut HashMap<u32, Vec<u8>>,
) -> Option<InboundFrame> {
    let Some(packet) = parse_mesh_packet(data) else {
        debug!(
            len = data.len(),
            head = %hex::encode(&data[..data.len().min(16)]),
            "Meshtastic FromRadio.packet did not parse into a decoded MeshPacket"
        );
        return None;
    };
    if packet_is_stale(packet.rx_time) {
        debug!(
            from = ?packet.from.map(|n| format!("!{:08x}", n)),
            rx_time = ?packet.rx_time,
            "Dropping stale Meshtastic packet from radio backlog"
        );
        return None;
    }
    let from = packet.from.unwrap_or(0);
    if Some(from) == local_node_num {
        debug!(
            from = format!("!{:08x}", from),
            "Ignoring Meshtastic local echo packet"
        );
        return None;
    }

    // Update per-contact bookkeeping (signal quality, position) for EVERY
    // heard packet, not just text ones — a node's RSSI/SNR/location should
    // reflect the most recently heard packet of any kind.
    if let Some(pk) = packet.public_key.as_ref() {
        peer_pubkeys.entry(from).or_insert_with(|| pk.clone());
    }
    let contact = contacts.entry(from).or_insert_with(|| ParsedContact {
        public_key_hex: hex::encode(synthetic_pubkey(from)),
        advert_name: format!("Meshtastic !{:08x}", from),
        last_advert: 0,
        contact_type: 1,
        path_len: 0xff,
        flags: 0,
        // Stamped fresh from `peer_pubkeys` in `get_contacts`.
        pkc_capable: false,
        rssi: None,
        snr: None,
        lat: None,
        lon: None,
        arch_pubkey_hex: None,
    });
    if packet.rx_rssi.is_some() {
        contact.rssi = packet.rx_rssi.map(|v| v as i16);
    }
    if packet.rx_snr.is_some() {
        contact.snr = packet.rx_snr;
    }

    if packet.portnum == POSITION_APP {
        if let Some((lat, lon)) = parse_position_lat_lon(&packet.payload) {
            debug!(
                from = format!("!{:08x}", from),
                lat, lon, "Meshtastic position update"
            );
            contact.lat = Some(lat);
            contact.lon = Some(lon);
        }
        return None;
    }

    if packet.portnum != TEXT_MESSAGE_APP || packet.payload.is_empty() {
        debug!(
            from = format!("!{:08x}", from),
            portnum = packet.portnum,
            payload_len = packet.payload.len(),
            pki = packet.pki_encrypted,
            "Meshtastic packet ignored because it is not a text payload"
        );
        return None;
    }
    info!(
        from = format!("!{:08x}", from),
        len = packet.payload.len(),
        pki = packet.pki_encrypted,
        "Meshtastic received text packet over the air"
    );
    if packet.pki_encrypted {
        debug!(
            node = from,
            "Meshtastic DM received end-to-end encrypted (PKI)"
        );
    }
    let from_key = synthetic_pubkey(from);

    // Channel broadcast (e.g. the default public LongFast channel, or any other
    // channel slot): `to == BROADCAST_NUM`. File it under the channel thread —
    // NOT a 1:1 DM with the sender — so it shows in the public/channel view,
    // while still carrying the sender prefix so the listener can attribute each
    // message to who sent it. Without this, every public-channel message was
    // scattered into per-sender DM threads and the public channel looked dead.
    if packet.to == Some(BROADCAST_NUM) {
        let mut data = Vec::with_capacity(7 + packet.payload.len());
        data.push(packet.channel); // channel index (0 = primary/public)
        data.extend_from_slice(&from_key[..6]); // sender pubkey prefix
        data.extend_from_slice(&packet.payload);
        return Some(InboundFrame {
            code: super::protocol::RESP_MESHTASTIC_CHANNEL_TEXT,
            data,
            bytes_consumed: 0,
        });
    }

    let mut payload = Vec::with_capacity(15 + packet.payload.len());
    payload.push(0); // SNR unknown
    payload.extend_from_slice(&[0, 0]); // reserved
    payload.extend_from_slice(&from_key[..6]);
    payload.push(0xff); // unknown/flood path
    payload.push(0); // text type
    payload.extend_from_slice(&packet.rx_time.unwrap_or_else(now_unix_secs).to_le_bytes());
    payload.extend_from_slice(&packet.payload);
    Some(InboundFrame {
        code: if packet.pki_encrypted {
            super::protocol::RESP_CONTACT_MSG_V3_E2E
        } else {
            super::protocol::RESP_CONTACT_MSG_V3
        },
        data: payload,
        bytes_consumed: 0,
    })
}

fn packet_is_stale(rx_time: Option<u32>) -> bool {
    let Some(rx_time) = rx_time else {
        return false;
    };
    // Radios without GPS/RTC can report tiny nonzero epoch values until their
    // clock is set. Treat those as unknown, not stale, or live LoRa packets from
    // stock Meshtastic peers disappear before reaching mesh.messages.
    if rx_time < PLAUSIBLE_RX_EPOCH_SECS {
        return false;
    }
    let now = now_unix_secs();
    if now < PLAUSIBLE_RX_EPOCH_SECS || rx_time > now.saturating_add(60) {
        return false;
    }
    rx_time.saturating_add(STALE_RX_SECS) < now
}

fn decode_serial_frame(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    loop {
        let start = buf.windows(2).position(|w| w == [START1, START2])?;
        if start > 0 {
            buf.drain(..start);
        }
        if buf.len() < 4 {
            return None;
        }
        let len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        if len == 0 || len > FROM_RADIO_MAX {
            debug!(
                len,
                head = %hex::encode(&buf[..buf.len().min(16)]),
                "Discarding invalid Meshtastic serial frame marker"
            );
            buf.drain(..1);
            continue;
        }
        if buf.len() < 4 + len {
            return None;
        }
        let payload = buf[4..4 + len].to_vec();
        if decode_top_level_variant(&payload).is_none() {
            debug!(
                len,
                head = %hex::encode(&buf[..buf.len().min(16)]),
                "Discarding invalid Meshtastic serial frame payload"
            );
            buf.drain(..1);
            continue;
        }
        buf.drain(..4 + len);
        return Some(payload);
    }
}

fn encode_want_config() -> Vec<u8> {
    encode_varint_field(TO_RADIO_WANT_CONFIG_ID, 1)
}

/// Derive a Meshtastic short_name (≤4 chars, the label shown on node icons) from
/// the human node name: the first few alphanumeric characters, upper-cased.
/// Returns `None` when the name has no usable alphanumeric characters.
fn derive_short_name(name: &str) -> Option<String> {
    let short: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(MESHTASTIC_SHORT_NAME_MAX)
        .collect::<String>()
        .to_uppercase();
    if short.is_empty() {
        None
    } else {
        Some(short)
    }
}

fn encode_heartbeat() -> Vec<u8> {
    encode_to_radio_variant(TO_RADIO_HEARTBEAT, &[])
}

/// Extract `LoRaConfig.region` from a `Config` message, returning the region
/// code. Returns `Some(REGION_UNSET)` when the LoRa block is present but has no
/// region field (a fresh radio), and `None` when this Config carries a
/// non-LoRa variant (device/position/…) so the caller keeps the prior value.
/// Returns `(region, modem_preset)` from a `Config.lora` block. `modem_preset`
/// defaults to `LORA_MODEM_PRESET_LONG_FAST` when the field is absent — a
/// fresh/never-configured radio reports no preset field at all, and treating
/// that as "already correct" (rather than "unknown, needs fixing") avoids a
/// spurious reboot before the operator-region provisioning step even runs.
fn parse_config_lora_region(data: &[u8]) -> Option<(u32, u32)> {
    let mut idx = 0;
    while idx < data.len() {
        let (field, value, next) = next_field(data, idx)?;
        idx = next;
        if field == CONFIG_LORA_FIELD {
            if let FieldValue::Bytes(b) = value {
                let mut j = 0;
                let mut region = REGION_UNSET;
                let mut modem_preset = LORA_MODEM_PRESET_LONG_FAST as u32;
                while j < b.len() {
                    let (lf, lv, ln) = next_field(b, j)?;
                    j = ln;
                    match (lf, lv) {
                        (LORA_REGION_FIELD, FieldValue::Varint(v)) => region = v as u32,
                        (LORA_MODEM_PRESET_FIELD, FieldValue::Varint(v)) => modem_preset = v as u32,
                        _ => {}
                    }
                }
                return Some((region, modem_preset));
            }
        }
    }
    None
}

/// True when the radio's primary channel is the off-the-shelf DEFAULT public
/// channel (empty name + the default key, in either its 1-byte or expanded
/// form). Used so we only rewrite the primary when it's been clobbered (e.g. an
/// older archy that set "archipelago" as primary) — never on a stock radio.
fn primary_is_default(primary: &Option<(String, Vec<u8>)>) -> bool {
    match primary {
        Some((name, psk)) => {
            name.is_empty()
                && (psk.as_slice() == DEFAULT_PSK_BYTE || psk.as_slice() == DEFAULT_PSK_EXPANDED)
        }
        None => false,
    }
}

/// Extract `(index, role, name, psk)` from a `Channel` message. The caller
/// stores the primary (slot 0) and our secondary slot separately so it can keep
/// both the public default channel and the private archipelago channel in sync.
fn parse_channel(data: &[u8]) -> Option<(u64, u64, String, Vec<u8>)> {
    let mut index = 0u64;
    let mut role = 0u64;
    let mut name = String::new();
    let mut psk = Vec::new();
    let mut idx = 0;
    while idx < data.len() {
        let (field, value, next) = next_field(data, idx)?;
        idx = next;
        match (field, value) {
            (1, FieldValue::Varint(v)) => index = v,
            (3, FieldValue::Varint(v)) => role = v,
            (2, FieldValue::Bytes(b)) => {
                let mut j = 0;
                while j < b.len() {
                    let (sf, sv, sn) = next_field(b, j)?;
                    j = sn;
                    match (sf, sv) {
                        (2, FieldValue::Bytes(p)) => psk = p.to_vec(),
                        (3, FieldValue::Bytes(n)) => name = String::from_utf8_lossy(n).to_string(),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Some((index, role, name, psk))
}

/// Derive the 32-byte channel PSK deterministically from the channel name, so
/// every archy node configured with the same `channel_name` converges on the
/// exact same primary channel (identical hash) and meshes automatically.
fn derive_channel_psk(channel_name: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"archipelago-mesh:");
    hasher.update(channel_name.as_bytes());
    hasher.finalize().to_vec()
}

/// Map a Meshtastic `RegionCode` name (as set in `mesh-config.json`, e.g.
/// "EU_868", "US", "ANZ") to its protobuf enum value. Case-insensitive.
/// Returns `None` for an unrecognised name so we never write a bogus region.
/// Inverse of `region_name_to_code` — for showing a probed radio's current
/// region to the user (hot-swap setup modal).
pub(crate) fn region_code_to_name(code: u32) -> Option<&'static str> {
    Some(match code {
        0 => "UNSET",
        1 => "US",
        2 => "EU_433",
        3 => "EU_868",
        4 => "CN",
        5 => "JP",
        6 => "ANZ",
        7 => "KR",
        8 => "TW",
        9 => "RU",
        10 => "IN",
        11 => "NZ_865",
        12 => "TH",
        13 => "LORA_24",
        14 => "UA_433",
        15 => "UA_868",
        16 => "MY_433",
        17 => "MY_919",
        18 => "SG_923",
        19 => "PH_433",
        20 => "PH_868",
        21 => "PH_915",
        22 => "ANZ_433",
        _ => return None,
    })
}

/// Meshtastic `Config.LoRaConfig.ModemPreset` enum names, for display.
pub(crate) fn modem_preset_name(code: u32) -> Option<&'static str> {
    Some(match code {
        0 => "LONG_FAST",
        1 => "LONG_SLOW",
        2 => "VERY_LONG_SLOW",
        3 => "MEDIUM_SLOW",
        4 => "MEDIUM_FAST",
        5 => "SHORT_SLOW",
        6 => "SHORT_FAST",
        7 => "LONG_MODERATE",
        8 => "SHORT_TURBO",
        _ => return None,
    })
}

pub(crate) fn region_name_to_code(name: &str) -> Option<u32> {
    Some(match name.trim().to_uppercase().as_str() {
        "UNSET" => 0,
        "US" => 1,
        "EU_433" => 2,
        "EU_868" | "EU868" => 3,
        "CN" => 4,
        "JP" => 5,
        "ANZ" => 6,
        "KR" => 7,
        "TW" => 8,
        "RU" => 9,
        "IN" => 10,
        "NZ_865" => 11,
        "TH" => 12,
        "LORA_24" => 13,
        "UA_433" => 14,
        "UA_868" => 15,
        "MY_433" => 16,
        "MY_919" => 17,
        "SG_923" => 18,
        "PH_433" => 19,
        "PH_868" => 20,
        "PH_915" => 21,
        "ANZ_433" => 22,
        _ => return None,
    })
}

fn encode_to_radio_variant(field: u64, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_len_field(field, bytes, &mut out);
    out
}

fn encode_mesh_packet(to: u32, portnum: u32, payload: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    encode_varint_field_into(1, portnum as u64, &mut decoded);
    encode_len_field(2, payload, &mut decoded);

    let mut packet = Vec::new();
    encode_fixed32_field(2, to, &mut packet);
    encode_len_field(4, &decoded, &mut packet);
    encode_fixed32_field(6, next_packet_id(), &mut packet);
    encode_fixed32_field(7, now_unix_secs(), &mut packet);
    // Meshtastic treats an unset hop_limit as zero, i.e. direct-neighbor only.
    // Set a normal mesh hop limit so stock-device DMs can route beyond one hop.
    encode_varint_field_into(9, 3, &mut packet);
    if to != BROADCAST_NUM {
        encode_varint_field_into(10, 1, &mut packet);
    }
    packet
}

fn decode_top_level_variant(buf: &[u8]) -> Option<(u64, &[u8])> {
    let mut idx = 0;
    while idx < buf.len() {
        let (key, n) = read_varint(&buf[idx..])?;
        idx += n;
        let field = key >> 3;
        match key & 0x07 {
            0 => {
                let (_, n) = read_varint(&buf[idx..])?;
                idx += n;
                if matches!(field, FROM_RADIO_CONFIG_COMPLETE_ID | FROM_RADIO_REBOOTED) {
                    return Some((field, &[]));
                }
            }
            2 => {
                let (len, n) = read_varint(&buf[idx..])?;
                idx += n;
                let end = idx.checked_add(len as usize)?;
                if end > buf.len() {
                    return None;
                }
                if matches!(
                    field,
                    FROM_RADIO_PACKET
                        | FROM_RADIO_MY_INFO
                        | FROM_RADIO_NODE_INFO
                        | FROM_RADIO_CONFIG
                        | FROM_RADIO_CHANNEL
                        | FROM_RADIO_QUEUE_STATUS
                        | FROM_RADIO_XMODEM_PACKET
                        | FROM_RADIO_METADATA
                        | FROM_RADIO_MQTT_CLIENT_PROXY_MESSAGE
                        | FROM_RADIO_FILE_INFO
                        | FROM_RADIO_CLIENT_NOTIFICATION
                        | FROM_RADIO_DEVICE_UI_CONFIG
                        | FROM_RADIO_LOCKDOWN_STATUS
                        | FROM_RADIO_REGION_PRESETS
                ) {
                    return Some((field, &buf[idx..end]));
                }
                idx = end;
            }
            _ => return None,
        }
    }
    None
}

fn parse_my_info(data: &[u8]) -> Option<(u32, Option<String>)> {
    let mut idx = 0;
    let mut node_num = None;
    let mut user_id = None;
    while idx < data.len() {
        let (field, value, next) = next_field(data, idx)?;
        idx = next;
        match (field, value) {
            (1, FieldValue::Varint(v)) => node_num = Some(v as u32),
            (1, FieldValue::Fixed32(v)) => node_num = Some(v),
            (3, FieldValue::Bytes(b)) => user_id = parse_user(b).and_then(|u| u.id),
            _ => {}
        }
    }
    node_num.map(|n| (n, user_id))
}

struct ParsedNode {
    num: u32,
    id: Option<String>,
    long_name: Option<String>,
    short_name: Option<String>,
    last_heard: Option<u32>,
    public_key: Option<Vec<u8>>,
}

fn parse_node_info(data: &[u8]) -> Option<ParsedNode> {
    let mut idx = 0;
    let mut node = ParsedNode {
        num: 0,
        id: None,
        long_name: None,
        short_name: None,
        last_heard: None,
        public_key: None,
    };
    while idx < data.len() {
        let (field, value, next) = next_field(data, idx)?;
        idx = next;
        match (field, value) {
            (1, FieldValue::Varint(v)) => node.num = v as u32,
            (1, FieldValue::Fixed32(v)) => node.num = v,
            (2, FieldValue::Bytes(b)) => {
                if let Some(user) = parse_user(b) {
                    node.id = user.id;
                    node.long_name = user.long_name;
                    node.short_name = user.short_name;
                    node.public_key = user.public_key;
                }
            }
            (5, FieldValue::Fixed32(v)) => node.last_heard = Some(v),
            _ => {}
        }
    }
    if node.num == 0 {
        None
    } else {
        Some(node)
    }
}

struct ParsedUser {
    id: Option<String>,
    long_name: Option<String>,
    short_name: Option<String>,
    public_key: Option<Vec<u8>>,
}

fn parse_user(data: &[u8]) -> Option<ParsedUser> {
    let mut idx = 0;
    let mut user = ParsedUser {
        id: None,
        long_name: None,
        short_name: None,
        public_key: None,
    };
    while idx < data.len() {
        let (field, value, next) = next_field(data, idx)?;
        idx = next;
        match (field, value) {
            (1, FieldValue::Bytes(b)) => user.id = string_field(b),
            (2, FieldValue::Bytes(b)) => user.long_name = string_field(b),
            (3, FieldValue::Bytes(b)) => user.short_name = string_field(b),
            // User.public_key (field 8): the peer's Curve25519 key. Its presence
            // means the radio can PKC-encrypt DMs to this node end-to-end.
            (8, FieldValue::Bytes(b)) if !b.is_empty() => user.public_key = Some(b.to_vec()),
            _ => {}
        }
    }
    Some(user)
}

struct ParsedPacket {
    from: Option<u32>,
    /// MeshPacket.to (field 2): the destination node, or `BROADCAST_NUM` for a
    /// channel broadcast. Distinguishes a directed DM from a public/channel
    /// message so each lands in the right thread.
    to: Option<u32>,
    /// MeshPacket.channel (field 3): the channel index a broadcast arrived on
    /// (0 = primary / default public LongFast).
    channel: u8,
    portnum: u32,
    payload: Vec<u8>,
    #[allow(dead_code)]
    id: Option<u32>,
    rx_time: Option<u32>,
    /// MeshPacket.pki_encrypted (field 17): the firmware decrypted this packet
    /// with the PKI (Curve25519) key, i.e. it arrived end-to-end encrypted
    /// rather than via the shared channel PSK.
    pki_encrypted: bool,
    /// MeshPacket.public_key (field 16): the sender's key, carried on PKC DMs.
    public_key: Option<Vec<u8>>,
    /// MeshPacket.rx_snr (field 8, float) — signal-to-noise ratio in dB, as
    /// measured by the receiving radio. Field numbers confirmed against the
    /// canonical `meshtastic/protobufs` mesh.proto, not guessed.
    rx_snr: Option<f32>,
    /// MeshPacket.rx_rssi (field 12, int32) — received signal strength in dBm.
    rx_rssi: Option<i32>,
}

fn parse_mesh_packet(data: &[u8]) -> Option<ParsedPacket> {
    let mut idx = 0;
    let mut from = None;
    let mut to = None;
    let mut channel = 0u8;
    let mut decoded = None;
    let mut id = None;
    let mut rx_time = None;
    let mut pki_encrypted = false;
    let mut public_key = None;
    let mut rx_snr = None;
    let mut rx_rssi = None;
    while idx < data.len() {
        let (field, value, next) = next_field(data, idx)?;
        idx = next;
        match (field, value) {
            (1, FieldValue::Fixed32(v)) => from = Some(v),
            (2, FieldValue::Fixed32(v)) => to = Some(v),
            (3, FieldValue::Varint(v)) => channel = v as u8,
            (4, FieldValue::Bytes(b)) => decoded = Some(b),
            (6, FieldValue::Fixed32(v)) => id = Some(v),
            (7, FieldValue::Fixed32(v)) if v != 0 => rx_time = Some(v),
            // float wire type (5) decodes as a 32-bit fixed value; reinterpret
            // the bits as f32 rather than treating them as an integer.
            (8, FieldValue::Fixed32(v)) => rx_snr = Some(f32::from_bits(v)),
            (12, FieldValue::Varint(v)) => rx_rssi = Some(v as i32),
            (16, FieldValue::Bytes(b)) if !b.is_empty() => public_key = Some(b.to_vec()),
            (17, FieldValue::Varint(v)) => pki_encrypted = v != 0,
            _ => {}
        }
    }
    let decoded = decoded?;
    let mut didx = 0;
    let mut portnum = 0;
    let mut payload = Vec::new();
    while didx < decoded.len() {
        let (field, value, next) = next_field(decoded, didx)?;
        didx = next;
        match (field, value) {
            (1, FieldValue::Varint(v)) => portnum = v as u32,
            (2, FieldValue::Bytes(b)) => payload = b.to_vec(),
            _ => {}
        }
    }
    Some(ParsedPacket {
        from,
        to,
        channel,
        portnum,
        payload,
        id,
        rx_time,
        pki_encrypted,
        public_key,
        rx_snr,
        rx_rssi,
    })
}

/// Decode a Meshtastic `Position` protobuf's `latitude_i`/`longitude_i`
/// (fields 1/2, both `sfixed32` — signed, wire type 5/Fixed32) into degrees.
/// Field numbers confirmed against the canonical `meshtastic/protobufs`
/// mesh.proto, not guessed. Returns `None` if either field is absent — a
/// Position packet without both isn't a usable fix.
fn parse_position_lat_lon(data: &[u8]) -> Option<(f64, f64)> {
    let mut idx = 0;
    let mut lat_i = None;
    let mut lon_i = None;
    while idx < data.len() {
        let (field, value, next) = next_field(data, idx)?;
        idx = next;
        match (field, value) {
            (1, FieldValue::Fixed32(v)) => lat_i = Some(v as i32),
            (2, FieldValue::Fixed32(v)) => lon_i = Some(v as i32),
            _ => {}
        }
    }
    match (lat_i, lon_i) {
        (Some(lat), Some(lon)) => Some((lat as f64 * 1e-7, lon as f64 * 1e-7)),
        _ => None,
    }
}

fn now_unix_secs() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0)
}

fn next_packet_id() -> u32 {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let ctr = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    nanos ^ ctr.rotate_left(16)
}

enum FieldValue<'a> {
    Varint(u64),
    Fixed32(u32),
    Bytes(&'a [u8]),
}

fn next_field(buf: &[u8], idx: usize) -> Option<(u64, FieldValue<'_>, usize)> {
    let (key, n) = read_varint(&buf[idx..])?;
    let field = key >> 3;
    let mut pos = idx + n;
    match key & 0x07 {
        0 => {
            let (v, n) = read_varint(&buf[pos..])?;
            pos += n;
            Some((field, FieldValue::Varint(v), pos))
        }
        2 => {
            let (len, n) = read_varint(&buf[pos..])?;
            pos += n;
            let end = pos.checked_add(len as usize)?;
            if end > buf.len() {
                return None;
            }
            Some((field, FieldValue::Bytes(&buf[pos..end]), end))
        }
        5 => {
            let end = pos.checked_add(4)?;
            if end > buf.len() {
                None
            } else {
                let value =
                    u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
                Some((field, FieldValue::Fixed32(value), end))
            }
        }
        1 => {
            let end = pos.checked_add(8)?;
            if end > buf.len() {
                None
            } else {
                Some((field, FieldValue::Bytes(&buf[pos..end]), end))
            }
        }
        wire => {
            warn!(wire, "Unsupported Meshtastic protobuf wire type");
            None
        }
    }
}

fn read_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let mut out = 0u64;
    for (i, b) in buf.iter().copied().enumerate().take(10) {
        out |= ((b & 0x7f) as u64) << (7 * i);
        if b & 0x80 == 0 {
            return Some((out, i + 1));
        }
    }
    None
}

fn encode_varint_field(field: u64, value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    encode_varint_field_into(field, value, &mut out);
    out
}

fn encode_varint_field_into(field: u64, value: u64, out: &mut Vec<u8>) {
    write_varint((field << 3) | 0, out);
    write_varint(value, out);
}

fn encode_len_field(field: u64, bytes: &[u8], out: &mut Vec<u8>) {
    write_varint((field << 3) | 2, out);
    write_varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

fn encode_fixed32_field(field: u64, value: u32, out: &mut Vec<u8>) {
    write_varint((field << 3) | 5, out);
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn string_field(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes).ok().map(|s| s.to_string())
}

fn synthetic_pubkey(node_num: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..4].copy_from_slice(&node_num.to_le_bytes());
    out[4..15].copy_from_slice(b"meshtastic:");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::protocol;

    fn serial_frame(payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.push(START1);
        frame.push(START2);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn decode_serial_frame_skips_false_marker_with_impossible_length() {
        let valid_payload = encode_varint_field(FROM_RADIO_CONFIG_COMPLETE_ID, 1);
        let mut buf = vec![b'l', b'o', b'g', START1, START2, 0xff, 0xff, b'x'];
        buf.extend_from_slice(&serial_frame(&valid_payload));

        let decoded = decode_serial_frame(&mut buf).expect("valid frame after false marker");
        assert_eq!(decoded, valid_payload);
    }

    #[test]
    fn decode_serial_frame_skips_false_marker_with_invalid_payload() {
        let valid_payload = encode_varint_field(FROM_RADIO_CONFIG_COMPLETE_ID, 1);
        let mut buf = vec![START1, START2, 0x00, 0x03, b'b', b'a', b'd'];
        buf.extend_from_slice(&serial_frame(&valid_payload));

        let decoded = decode_serial_frame(&mut buf).expect("valid frame after invalid payload");
        assert_eq!(decoded, valid_payload);
    }

    #[test]
    fn decode_serial_frame_accepts_queue_status_variant() {
        let mut queue_status = Vec::new();
        encode_len_field(
            FROM_RADIO_QUEUE_STATUS,
            &[0x10, 0x0e, 0x18, 0x10],
            &mut queue_status,
        );
        let mut buf = serial_frame(&queue_status);

        let decoded =
            decode_serial_frame(&mut buf).expect("queue status is a valid FromRadio frame");
        assert_eq!(decoded, queue_status);
    }

    #[test]
    fn encode_mesh_packet_sets_nonzero_id_and_time() {
        let before = now_unix_secs();
        let packet = encode_mesh_packet(0x1122_3344, TEXT_MESSAGE_APP, b"hello");
        let after = now_unix_secs();
        let parsed = parse_mesh_packet(&packet).expect("packet should parse");

        assert_eq!(parsed.portnum, TEXT_MESSAGE_APP);
        assert_eq!(parsed.payload, b"hello");
        assert!(parsed.id.unwrap_or(0) != 0);
        let rx_time = parsed.rx_time.expect("rx_time should be set");
        assert!(rx_time >= before.saturating_sub(1));
        assert!(rx_time <= after.saturating_add(1));
    }

    #[test]
    fn parse_mesh_packet_decodes_rx_snr_and_rx_rssi() {
        // Field numbers confirmed against the canonical meshtastic/protobufs
        // mesh.proto: rx_snr=8 (float, fixed32 wire type), rx_rssi=12 (int32,
        // varint wire type) — hand-built here since encode_mesh_packet (an
        // OUTBOUND builder) never sets these inbound-only fields.
        let mut decoded = Vec::new();
        encode_varint_field_into(1, TEXT_MESSAGE_APP as u64, &mut decoded);
        encode_len_field(2, b"hi", &mut decoded);

        let mut packet = Vec::new();
        encode_fixed32_field(1, 0x1111_2222, &mut packet); // from
        encode_len_field(4, &decoded, &mut packet); // decoded
        encode_fixed32_field(8, (-7.5f32).to_bits(), &mut packet); // rx_snr
                                                                   // int32 rx_rssi=-92 dBm, protobuf varint-encodes a negative int32 as
                                                                   // the 10-byte two's-complement-extended varint; truncating back to
                                                                   // i32 after decode recovers the original value (see parse_mesh_packet's
                                                                   // `v as i32` cast — confirmed by this roundtrip, not just asserted).
        encode_varint_field_into(12, (-92i32) as u32 as u64, &mut packet);

        let parsed = parse_mesh_packet(&packet).expect("packet should parse");
        assert_eq!(parsed.rx_snr, Some(-7.5));
        assert_eq!(parsed.rx_rssi, Some(-92));
    }

    #[test]
    fn parse_position_lat_lon_decodes_sfixed32_degrees() {
        // Position.latitude_i/longitude_i = fields 1/2 (sfixed32), confirmed
        // against the canonical mesh.proto. New York City, roughly.
        let lat_i: i32 = 407_128_000; // 40.7128 degrees * 1e7
        let lon_i: i32 = -740_060_000; // -74.0060 degrees * 1e7
        let mut position = Vec::new();
        encode_fixed32_field(1, lat_i as u32, &mut position);
        encode_fixed32_field(2, lon_i as u32, &mut position);

        let (lat, lon) = parse_position_lat_lon(&position).expect("both fields present");
        assert!((lat - 40.7128).abs() < 1e-6);
        assert!((lon - (-74.0060)).abs() < 1e-6);
    }

    #[test]
    fn parse_position_lat_lon_none_when_incomplete() {
        let mut position = Vec::new();
        encode_fixed32_field(1, 0, &mut position); // only latitude, no longitude
        assert!(parse_position_lat_lon(&position).is_none());
    }

    #[test]
    fn parse_config_lora_region_decodes_region_and_modem_preset() {
        let mut lora = Vec::new();
        encode_varint_field_into(LORA_REGION_FIELD, 3, &mut lora); // some real region code
        encode_varint_field_into(LORA_MODEM_PRESET_FIELD, 4, &mut lora); // drifted preset

        let mut config = Vec::new();
        encode_len_field(CONFIG_LORA_FIELD, &lora, &mut config);

        let (region, preset) = parse_config_lora_region(&config).expect("lora config present");
        assert_eq!(region, 3);
        assert_eq!(preset, 4);
    }

    #[test]
    fn parse_config_lora_region_defaults_preset_to_long_fast_when_absent() {
        // A fresh/never-provisioned radio's Config.lora may carry a region
        // with no modem_preset field at all -- must default to "already
        // correct" (LONG_FAST), not "unknown/wrong", or ensure_lora_region
        // would try to fix a value it never actually observed.
        let mut lora = Vec::new();
        encode_varint_field_into(LORA_REGION_FIELD, 3, &mut lora);

        let mut config = Vec::new();
        encode_len_field(CONFIG_LORA_FIELD, &lora, &mut config);

        let (region, preset) = parse_config_lora_region(&config).expect("lora config present");
        assert_eq!(region, 3);
        assert_eq!(preset, LORA_MODEM_PRESET_LONG_FAST as u32);
    }

    #[test]
    fn packet_to_inbound_frame_updates_contact_signal_and_position_without_a_chat_frame() {
        let from = 0x0000_4444;
        let mut contacts = HashMap::new();
        let mut peer_pubkeys = HashMap::new();

        // A POSITION_APP packet carrying rx_snr/rx_rssi too.
        let mut position = Vec::new();
        encode_fixed32_field(1, (407_128_000i32) as u32, &mut position);
        encode_fixed32_field(2, (-740_060_000i32) as u32, &mut position);
        let mut decoded = Vec::new();
        encode_varint_field_into(1, POSITION_APP as u64, &mut decoded);
        encode_len_field(2, &position, &mut decoded);

        let mut packet = Vec::new();
        encode_fixed32_field(1, from, &mut packet);
        encode_fixed32_field(2, BROADCAST_NUM, &mut packet);
        encode_len_field(4, &decoded, &mut packet);
        encode_fixed32_field(7, 12_345, &mut packet);
        encode_fixed32_field(8, (-6.0f32).to_bits(), &mut packet);
        encode_varint_field_into(12, (-80i32) as u32 as u64, &mut packet);

        // A position packet isn't a chat message — no InboundFrame — but it
        // must still update the contact's signal/position bookkeeping.
        let frame =
            packet_to_inbound_frame(&packet, Some(0x1111_1111), &mut contacts, &mut peer_pubkeys);
        assert!(
            frame.is_none(),
            "POSITION_APP must not surface as a chat frame"
        );
        let contact = contacts.get(&from).expect("contact should be tracked");
        assert_eq!(contact.snr, Some(-6.0));
        assert_eq!(contact.rssi, Some(-80));
        assert!((contact.lat.unwrap() - 40.7128).abs() < 1e-6);
        assert!((contact.lon.unwrap() - (-74.0060)).abs() < 1e-6);
    }

    #[test]
    fn packet_to_inbound_frame_accepts_stock_peer_with_unset_clock() {
        let from = 0x0000_3ccc;
        let mut contacts = HashMap::new();
        let mut peer_pubkeys = HashMap::new();
        let mut decoded = Vec::new();
        encode_varint_field_into(1, TEXT_MESSAGE_APP as u64, &mut decoded);
        encode_len_field(2, b"hello from 3ccc", &mut decoded);

        let mut packet = Vec::new();
        encode_fixed32_field(1, from, &mut packet);
        encode_fixed32_field(2, BROADCAST_NUM, &mut packet);
        encode_len_field(4, &decoded, &mut packet);
        encode_fixed32_field(7, 12_345, &mut packet);

        let frame =
            packet_to_inbound_frame(&packet, Some(0x1111_1111), &mut contacts, &mut peer_pubkeys)
                .expect("live packet with unset radio clock must not be dropped");
        // A `to == BROADCAST_NUM` text is a channel broadcast (3ccc on public
        // LongFast), so it routes to the channel thread, carrying its sender.
        assert_eq!(frame.code, protocol::RESP_MESHTASTIC_CHANNEL_TEXT);
        assert_eq!(
            frame.data[0], 0,
            "no channel field set => primary/public (0)"
        );
        assert_eq!(&frame.data[1..7], &[0xcc, 0x3c, 0x00, 0x00, 0x6d, 0x65]);
        assert_eq!(&frame.data[7..], b"hello from 3ccc");
        assert!(contacts.contains_key(&from));
    }

    #[test]
    fn packet_to_inbound_frame_directed_dm_stays_a_contact_message() {
        // A text addressed directly to us (to == our node, not broadcast) must
        // remain a 1:1 DM, NOT get rerouted to a channel thread.
        let from = 0x0000_3ccc;
        let me = 0x1111_1111;
        let mut contacts = HashMap::new();
        let mut peer_pubkeys = HashMap::new();
        let mut decoded = Vec::new();
        encode_varint_field_into(1, TEXT_MESSAGE_APP as u64, &mut decoded);
        encode_len_field(2, b"direct hello", &mut decoded);

        let mut packet = Vec::new();
        encode_fixed32_field(1, from, &mut packet);
        encode_fixed32_field(2, me, &mut packet); // to == us, directed
        encode_len_field(4, &decoded, &mut packet);
        encode_fixed32_field(7, 12_345, &mut packet);

        let frame = packet_to_inbound_frame(&packet, Some(me), &mut contacts, &mut peer_pubkeys)
            .expect("directed DM must surface");
        assert_eq!(frame.code, protocol::RESP_CONTACT_MSG_V3);
        let (sender_prefix, payload, _snr) =
            protocol::parse_contact_msg_v3_raw(&frame.data).unwrap();
        assert_eq!(sender_prefix, "cc3c00006d65");
        assert_eq!(payload, b"direct hello");
    }

    #[test]
    fn packet_to_inbound_frame_accepts_recent_meshtastic_backlog() {
        let from = 0x433e_3ccc;
        let mut contacts = HashMap::new();
        let mut peer_pubkeys = HashMap::new();
        let mut decoded = Vec::new();
        encode_varint_field_into(1, TEXT_MESSAGE_APP as u64, &mut decoded);
        encode_len_field(2, b"recent backlog", &mut decoded);

        let mut packet = Vec::new();
        encode_fixed32_field(1, from, &mut packet);
        encode_fixed32_field(2, BROADCAST_NUM, &mut packet);
        encode_len_field(4, &decoded, &mut packet);
        encode_fixed32_field(7, now_unix_secs().saturating_sub(60 * 60), &mut packet);

        let frame =
            packet_to_inbound_frame(&packet, Some(0x1111_1111), &mut contacts, &mut peer_pubkeys)
                .expect("recent radio backlog must surface in mesh.messages");
        // Broadcast → channel frame: [channel_idx][sender_prefix(6)][text].
        assert_eq!(frame.code, protocol::RESP_MESHTASTIC_CHANNEL_TEXT);
        assert_eq!(&frame.data[1..7], &[0xcc, 0x3c, 0x3e, 0x43, 0x6d, 0x65]);
        assert_eq!(&frame.data[7..], b"recent backlog");
    }

    #[test]
    fn stale_filter_keeps_packets_from_radios_with_unset_clock() {
        assert!(!packet_is_stale(None));
        assert!(!packet_is_stale(Some(0)));
        assert!(!packet_is_stale(Some(12_345)));
    }

    #[test]
    fn stale_filter_drops_only_plausibly_old_packets() {
        let old = now_unix_secs().saturating_sub(STALE_RX_SECS + 60);
        if old >= PLAUSIBLE_RX_EPOCH_SECS {
            assert!(packet_is_stale(Some(old)));
        }
    }
}
