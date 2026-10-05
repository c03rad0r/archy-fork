// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Meshcore binary frame protocol: constants, encoding, decoding, command builders.
//!
//! Frame format (USB serial):
//! - Outbound (host -> device): `<` (0x3C) + 2-byte LE length + frame data
//! - Inbound (device -> host):  `>` (0x3E) + 2-byte LE length + frame data
//! - Baud: 115200, 8N1
//! - Max message payload: 160 bytes

use anyhow::Result;

// --- Frame markers ---
pub const OUTBOUND_MARKER: u8 = 0x3C; // '<' (host -> device)
pub const INBOUND_MARKER: u8 = 0x3E; // '>' (device -> host)

// --- Commands (host -> device) ---
pub const CMD_APP_START: u8 = 0x01;
pub const CMD_SEND_TXT_MSG: u8 = 0x02;
pub const CMD_SEND_CHANNEL_TXT_MSG: u8 = 0x03;
pub const CMD_GET_CONTACTS: u8 = 0x04;
pub const CMD_GET_DEVICE_TIME: u8 = 0x05;
pub const CMD_SET_DEVICE_TIME: u8 = 0x06;
pub const CMD_SEND_SELF_ADVERT: u8 = 0x07;
pub const CMD_SET_ADVERT_NAME: u8 = 0x08;
pub const CMD_SYNC_NEXT_MESSAGE: u8 = 0x0A;
/// CMD_RESET_PATH (0x0D): Tell the firmware to drop the stored route for
/// a contact and fall back to flood routing (out_path_len = 0xFF). Used to
/// unstick direct messages to contacts whose `path_len=0` means "no route
/// known" — without this, the firmware silently drops outbound TXT_MSG
/// frames to such contacts.
pub const CMD_RESET_PATH: u8 = 0x0D;
/// CMD_ADD_UPDATE_CONTACT (0x09): add or update a contact in the firmware
/// table. 144-byte frame (see `build_add_contact`).
pub const CMD_ADD_UPDATE_CONTACT: u8 = 0x09;
/// CMD_REMOVE_CONTACT (0x0F): `[0x0F][pub_key:32]` — delete a contact from the
/// firmware's persistent table (used by clear-all so wiped contacts actually
/// go away and only return when they re-advertise).
pub const CMD_REMOVE_CONTACT: u8 = 0x0F;
pub const CMD_SET_RADIO_PARAMS: u8 = 0x0B;
pub const CMD_SET_RADIO_TX_POWER: u8 = 0x0C;
pub const CMD_SET_TUNING_PARAMS: u8 = 0x15;
pub const CMD_DEVICE_QUERY: u8 = 0x16;
pub const CMD_GET_CHANNEL: u8 = 0x1F;
pub const CMD_SET_CHANNEL: u8 = 0x20;
pub const CMD_GET_STATS: u8 = 0x38;

// --- Response codes (device -> host, synchronous) ---
pub const RESP_OK: u8 = 0x00;
pub const RESP_ERR: u8 = 0x01;
pub const RESP_CONTACT_START: u8 = 0x02;
pub const RESP_CONTACT: u8 = 0x03;
pub const RESP_CONTACT_END: u8 = 0x04;
pub const RESP_SELF_INFO: u8 = 0x05;
pub const RESP_SENT: u8 = 0x06;
pub const RESP_CONTACT_MSG: u8 = 0x07;
pub const RESP_CHANNEL_MSG: u8 = 0x08;
pub const RESP_CURRENT_TIME: u8 = 0x09;
pub const RESP_NO_MORE_MESSAGES: u8 = 0x0A;
pub const RESP_CONTACT_URI: u8 = 0x0B;
pub const RESP_BATTERY: u8 = 0x0C;
pub const RESP_DEVICE_INFO: u8 = 0x0D;
pub const RESP_CONTACT_MSG_V3: u8 = 0x10;
pub const RESP_CHANNEL_MSG_V3: u8 = 0x11;
pub const RESP_CHANNEL_INFO: u8 = 0x12;
pub const RESP_STATS: u8 = 0x18;
/// Archipelago-internal synthetic response code used by the Meshtastic adapter
/// for text DMs that the firmware reports as PKI-encrypted. Meshcore firmware
/// never emits this code; it lets the shared listener persist the E2E badge
/// without changing the on-wire Meshcore frame format.
pub const RESP_CONTACT_MSG_V3_E2E: u8 = 0x13;
/// Archipelago-internal synthetic response code used by the Meshtastic adapter
/// for CHANNEL broadcast text (e.g. the default public LongFast channel). Unlike
/// the Meshcore `RESP_CHANNEL_MSG_V3` — which carries no sender — a Meshtastic
/// MeshPacket gives us the originating node, so the listener can both file the
/// message under the channel thread AND attribute it to its sender. Frame
/// layout: `[channel_idx: u8][sender_pubkey_prefix: 6 bytes][text…]`. Kept below
/// 0x80 so it is not mistaken for a device push notification; Meshcore never
/// emits it.
pub const RESP_MESHTASTIC_CHANNEL_TEXT: u8 = 0x70;

// --- Push notification codes (device -> host, async, >= 0x80) ---
pub const PUSH_CONTACT_ADVERT: u8 = 0x80;
pub const PUSH_PATH_UPDATE: u8 = 0x81;
pub const PUSH_ACK: u8 = 0x82;
pub const PUSH_MESSAGES_WAITING: u8 = 0x83;
pub const PUSH_RAW_DATA: u8 = 0x84;
pub const PUSH_LOG_DATA: u8 = 0x88;
pub const PUSH_NEW_CONTACT: u8 = 0x8A;

// --- Error codes ---
pub const ERR_UNSUPPORTED_CMD: u8 = 0x01;
pub const ERR_NOT_FOUND: u8 = 0x02;
pub const ERR_TABLE_FULL: u8 = 0x03;
pub const ERR_BAD_STATE: u8 = 0x04;
pub const ERR_FILE_IO: u8 = 0x05;
pub const ERR_ILLEGAL_ARG: u8 = 0x06;

/// Maximum payload size for a single LoRa message.
pub const MAX_MESSAGE_LEN: usize = 160;

/// Marker byte for "direct message wrapped as channel broadcast". Our
/// meshcore devices can hear each other's channel broadcasts (via
/// repeater flooding) but direct unicast frames don't reach between
/// archipelago nodes — so we emulate DMs by sending them on the shared
/// channel with a recipient pubkey-prefix header. Format:
///   `[DM_VIA_CHANNEL_MARKER][dest_pubkey_prefix(6B)][inner_payload…]`
/// The inner payload is whatever we would have sent directly — a typed
/// envelope, a chunked MC frame, or plain text.
pub const DM_VIA_CHANNEL_MARKER: u8 = 0xD1;

/// Minimum frame size: marker (1) + length (2) + command/response (1) = 4 bytes.
const MIN_FRAME_SIZE: usize = 4;

/// Protocol version we advertise during handshake.
const PROTOCOL_VERSION: u8 = 3;

// ─── Frame encoding ─────────────────────────────────────────────────────

/// Encode a command frame for sending to the device.
/// Returns: `>` + 2-byte LE length + data
pub fn encode_frame(data: &[u8]) -> Vec<u8> {
    let len = data.len() as u16;
    let mut frame = Vec::with_capacity(3 + data.len());
    frame.push(OUTBOUND_MARKER);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(data);
    frame
}

/// Result of parsing one inbound frame from the device.
#[derive(Debug)]
pub struct InboundFrame {
    /// Response or push notification code (first byte of payload).
    pub code: u8,
    /// Remaining payload after the code byte.
    pub data: Vec<u8>,
    /// Total bytes consumed from the buffer (for advancing read position).
    pub bytes_consumed: usize,
}

/// Try to parse one inbound frame from a buffer.
/// Returns `None` if the buffer doesn't contain a complete frame yet.
pub fn decode_frame(buf: &[u8]) -> Option<InboundFrame> {
    if buf.len() < MIN_FRAME_SIZE {
        return None;
    }

    // Find the inbound marker
    let start = buf.iter().position(|&b| b == INBOUND_MARKER)?;
    let remaining = &buf[start..];

    if remaining.len() < 3 {
        return None;
    }

    let len = u16::from_le_bytes([remaining[1], remaining[2]]) as usize;
    let total = 3 + len; // marker + 2 length bytes + payload

    if remaining.len() < total {
        return None; // incomplete frame
    }

    if len == 0 {
        return None; // empty payload is invalid
    }

    let payload = &remaining[3..total];
    let code = payload[0];
    let data = payload[1..].to_vec();

    Some(InboundFrame {
        code,
        data,
        bytes_consumed: start + total,
    })
}

// ─── Command builders ───────────────────────────────────────────────────

/// CMD_DEVICE_QUERY (0x16): Query device capabilities and negotiate protocol version.
pub fn build_device_query() -> Vec<u8> {
    encode_frame(&[CMD_DEVICE_QUERY, PROTOCOL_VERSION])
}

/// CMD_APP_START (0x01): Initialize communication session.
/// Format matches official meshcore_py: [0x01][version][padded_name]
/// The official library sends: b"\x01\x03      mccli"
pub fn build_app_start(app_name: &str) -> Vec<u8> {
    let mut data = vec![CMD_APP_START, PROTOCOL_VERSION];
    // Pad name to 6 chars minimum (matching official library behavior)
    let name_bytes = app_name.as_bytes();
    let padded_len = name_bytes.len().max(6);
    let len = padded_len.min(32);
    // Pad with spaces if name is shorter than 6 chars
    for i in 0..len {
        if i < name_bytes.len() {
            data.push(name_bytes[i]);
        } else {
            data.push(b' ');
        }
    }
    encode_frame(&data)
}

/// CMD_SET_DEVICE_TIME (0x06): Sync device clock with Unix timestamp.
pub fn build_set_device_time(unix_secs: u64) -> Vec<u8> {
    let mut data = vec![CMD_SET_DEVICE_TIME];
    data.extend_from_slice(&(unix_secs as u32).to_le_bytes());
    encode_frame(&data)
}

/// CMD_SET_RADIO_PARAMS (0x0B): set the LoRa PHY config. The device reboots to
/// apply. `freq_field` and `bw_field` are the raw firmware fields (freq =
/// MHz×1000 e.g. 869618 for 869.618 MHz; bw = kHz×1000 e.g. 62500 for 62.5 kHz);
/// `sf` is 5..=12 and `cr` is 5..=8. Wire format verified against the MeshCore
/// companion firmware handler (`examples/companion_radio/MyMesh.cpp`,
/// `CMD_SET_RADIO_PARAMS`): `[11][freq:u32 LE][bw:u32 LE][sf:u8][cr:u8]`. The
/// same fields (same units) come back in the SELF_INFO reply, so a caller can
/// read them to detect drift. Values outside the firmware's accepted ranges are
/// rejected by the device (it replies with an error frame), not clamped here.
pub fn build_set_radio_params(freq_field: u32, bw_field: u32, sf: u8, cr: u8) -> Vec<u8> {
    let mut data = vec![CMD_SET_RADIO_PARAMS];
    data.extend_from_slice(&freq_field.to_le_bytes());
    data.extend_from_slice(&bw_field.to_le_bytes());
    data.push(sf);
    data.push(cr);
    encode_frame(&data)
}

/// CMD_SET_ADVERT_NAME (0x08): Set the node's advertised name on the mesh.
pub fn build_set_advert_name(name: &str) -> Vec<u8> {
    let mut data = vec![CMD_SET_ADVERT_NAME];
    let name_bytes = name.as_bytes();
    let len = name_bytes.len().min(32);
    data.extend_from_slice(&name_bytes[..len]);
    encode_frame(&data)
}

/// CMD_SEND_TXT_MSG (0x02): Send a text message to a specific contact.
/// Destination is the first 6 bytes of the contact's public key (hex decoded).
/// Format: 0x02 + 0x00 (txt_type) + attempt(1B) + timestamp(4B LE) + dest_prefix(6B) + text
pub fn build_send_text(dest_pubkey_prefix: &[u8; 6], msg: &[u8]) -> Result<Vec<u8>> {
    if msg.len() > MAX_MESSAGE_LEN {
        anyhow::bail!(
            "Message too large for LoRa: {} bytes (max {})",
            msg.len(),
            MAX_MESSAGE_LEN
        );
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;

    let mut data = vec![CMD_SEND_TXT_MSG, 0x00, 0x00]; // cmd + txt_type=0 + attempt=0
    data.extend_from_slice(&timestamp.to_le_bytes());
    data.extend_from_slice(dest_pubkey_prefix);
    data.extend_from_slice(msg);
    Ok(encode_frame(&data))
}

/// CMD_SEND_CHANNEL_TXT_MSG (0x03): Broadcast a text message on a channel.
/// Frame layout per meshcore companion protocol:
///   `[0x03][txt_type=0][channel][timestamp_le32][text…]`
/// The txt_type and timestamp fields are mandatory — without them the
/// firmware rejects the command with ERR_UNSUPPORTED.
pub fn build_send_channel_text(channel: u8, msg: &[u8]) -> Result<Vec<u8>> {
    if msg.len() > MAX_MESSAGE_LEN {
        anyhow::bail!(
            "Message too large for LoRa: {} bytes (max {})",
            msg.len(),
            MAX_MESSAGE_LEN
        );
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0);
    let mut data = vec![CMD_SEND_CHANNEL_TXT_MSG, 0x00, channel];
    data.extend_from_slice(&timestamp.to_le_bytes());
    data.extend_from_slice(msg);
    Ok(encode_frame(&data))
}

/// CMD_GET_CONTACTS (0x04): Request the contact list from the device.
pub fn build_get_contacts() -> Vec<u8> {
    encode_frame(&[CMD_GET_CONTACTS])
}

/// CMD_RESET_PATH (0x0D): `[0x0D][pub_key:32]`. Clears the stored route
/// for a contact so subsequent sends route via flood instead of being
/// silently dropped.
pub fn build_reset_path(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut data = vec![CMD_RESET_PATH];
    data.extend_from_slice(pubkey);
    encode_frame(&data)
}

/// CMD_REMOVE_CONTACT (0x0F): `[0x0F][pub_key:32]`. Removes the contact from
/// the firmware's persistent contact table.
pub fn build_remove_contact(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut data = vec![CMD_REMOVE_CONTACT];
    data.extend_from_slice(pubkey);
    encode_frame(&data)
}

/// CMD_ADD_UPDATE_CONTACT (0x09): add/update a contact. 144-byte body:
/// `[0x09][pub_key:32][type:1][flags:1][out_path_len:1][out_path:64][name:32]
///  [last_advert:4 LE][adv_lat:4 LE][adv_lon:4 LE]`.
/// `name` is zero-padded to 32 bytes (the firmware fills it from the heard
/// advert on its side too, so an empty name still resolves on get-contacts).
pub fn build_add_contact(
    pubkey: &[u8; 32],
    contact_type: u8,
    flags: u8,
    out_path_len: u8,
    name: &str,
    last_advert: u32,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(144);
    data.push(CMD_ADD_UPDATE_CONTACT);
    data.extend_from_slice(pubkey); // 32
    data.push(contact_type); // 1
    data.push(flags); // 1
    data.push(out_path_len); // 1
    data.extend_from_slice(&[0u8; 64]); // out_path (64)
    let mut name_buf = [0u8; 32];
    let nb = name.as_bytes();
    let n = nb.len().min(32);
    name_buf[..n].copy_from_slice(&nb[..n]);
    data.extend_from_slice(&name_buf); // name (32)
    data.extend_from_slice(&last_advert.to_le_bytes()); // last_advert (4)
    data.extend_from_slice(&0i32.to_le_bytes()); // adv_lat (4)
    data.extend_from_slice(&0i32.to_le_bytes()); // adv_lon (4)
    encode_frame(&data)
}

/// CMD_SYNC_NEXT_MESSAGE (0x0A): Retrieve the next queued message.
pub fn build_sync_next_message() -> Vec<u8> {
    encode_frame(&[CMD_SYNC_NEXT_MESSAGE])
}

/// CMD_SEND_SELF_ADVERT (0x07): Broadcast our advertisement to the mesh.
pub fn build_send_self_advert() -> Vec<u8> {
    encode_frame(&[CMD_SEND_SELF_ADVERT])
}

/// CMD_GET_STATS (0x38): Request device statistics.
pub fn build_get_stats() -> Vec<u8> {
    encode_frame(&[CMD_GET_STATS])
}

// ─── Response parsers ───────────────────────────────────────────────────

/// Decode a device/contact name from raw frame bytes, defensively.
///
/// Name fields sit at firmware-version-dependent offsets; when an offset
/// lands inside binary data (path/pubkey bytes), `from_utf8_lossy` used to
/// hand the UI replacement-character soup ("�\u{618}…"). Strict rules: valid
/// UTF-8 up to the first NUL, no control characters, at least one
/// non-whitespace char — anything else gets the caller's readable fallback.
fn decode_mesh_name(bytes: &[u8], fallback: &str) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    match std::str::from_utf8(&bytes[..end]) {
        Ok(s) => {
            let s = s.trim();
            if !s.is_empty() && s.chars().all(|c| !c.is_control()) {
                s.to_string()
            } else {
                fallback.to_string()
            }
        }
        Err(_) => fallback.to_string(),
    }
}

/// Parse RESP_DEVICE_INFO (0x0D) response.
/// Returns firmware version string and device capabilities.
pub fn parse_device_info(data: &[u8]) -> Result<(String, u16)> {
    // Official companion v3+ binary layout: protocol, half-capacity,
    // channels, PIN (4), build date (12), model (40), version (20).
    // Verified against companion-v1.17.1 MyMesh.cpp and Heltec V3 readback.
    if data
        .first()
        .is_some_and(|version| (3..=31).contains(version))
    {
        anyhow::ensure!(data.len() >= 79, "Truncated MeshCore device info");
        return Ok((
            decode_mesh_name(&data[59..79], "unknown"),
            u16::from(data[1]) * 2,
        ));
    }
    // Device info format varies by firmware version.
    // Minimum: firmware version string (null-terminated) + max_contacts (u16 LE)
    if data.is_empty() {
        anyhow::bail!("Empty device info response");
    }

    // Find null terminator for version string, or use all data as version
    let version_end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    let version = String::from_utf8_lossy(&data[..version_end]).to_string();

    let max_contacts = if data.len() > version_end + 2 {
        u16::from_le_bytes([data[version_end + 1], data[version_end + 2]])
    } else {
        100 // default
    };

    Ok((version, max_contacts))
}

/// Parse RESP_SELF_INFO (0x05) response.
/// Returns (node_id, advert_name).
pub fn parse_self_info(data: &[u8]) -> Result<(u32, String)> {
    if data.len() < 4 {
        anyhow::bail!("Self info response too short: {} bytes", data.len());
    }

    // Current companions send type/power/max-power, public key (32),
    // position (8), four preference bytes, RF parameters (10), then name.
    if data.len() >= 57 {
        let node_id = u32::from_le_bytes(data[3..7].try_into().unwrap());
        return Ok((
            node_id,
            decode_mesh_name(&data[57..], &format!("node-{node_id:08x}")),
        ));
    }
    let node_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

    // Name follows after fixed fields. A firmware whose fixed-field layout
    // differs would put binary here — decode defensively so the settings
    // panel never shows byte soup.
    let name_start = 4;
    let name = if data.len() > name_start {
        decode_mesh_name(&data[name_start..], &format!("node-{node_id:08x}"))
    } else {
        String::new()
    };

    Ok((node_id, name))
}

/// Parsed contact from RESP_CONTACT (0x03).
#[derive(Clone)]
pub struct ParsedContact {
    pub public_key_hex: String,
    pub advert_name: String,
    pub last_advert: u32,
    pub contact_type: u8,
    pub path_len: u8,
    pub flags: u8,
    /// Whether this contact is end-to-end (PKI / Curve25519) capable. Only the
    /// Meshtastic adapter sets this (true once we've learned the peer's real
    /// NodeInfo public key, so the firmware delivers DMs PKC-encrypted). Meshcore
    /// contacts leave it `false` — their E2E status is tracked per-message.
    pub pkc_capable: bool,
    /// Signal strength (dBm) / signal-to-noise ratio (dB) of the most recently
    /// heard packet from this contact. Meshtastic-only today (from
    /// `MeshPacket.rx_rssi`/`.rx_snr`); other transports leave these `None`.
    pub rssi: Option<i16>,
    pub snr: Option<f32>,
    /// Last known position, from a Meshtastic `POSITION_APP` broadcast
    /// (`Position.latitude_i`/`.longitude_i`, degrees). `None` until the
    /// contact has shared one.
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Archipelago ed25519 identity hex, when this transport carried it
    /// in-band with the contact announce itself (Reticulum only today — the
    /// RNS announce's app_data can embed an `ARCHY:n:` identity blob
    /// alongside the destination hash in the same event, so there's no
    /// ambiguity about which physical peer it belongs to). Meshcore/
    /// Meshtastic identity adverts go out on a separate channel and are
    /// correlated after the fact by `bind_federation_twins`'s advert_name
    /// matching instead, so they always leave this `None`.
    pub arch_pubkey_hex: Option<String>,
}

/// Parse RESP_CONTACT (0x03) response.
/// Format: 32B pubkey + 1B type + 1B flags + 1B path_len + 64B path + 32B name + 4B last_advert + 4B lat + 4B lon + 4B lastmod
pub fn parse_contact(data: &[u8]) -> Result<ParsedContact> {
    if data.len() < 34 {
        anyhow::bail!(
            "Contact response too short: {} bytes (need >= 34)",
            data.len()
        );
    }

    let public_key_hex = hex::encode(&data[0..32]);
    let contact_type = data[32];
    let flags = if data.len() > 33 { data[33] } else { 0 };
    let path_len = if data.len() > 34 { data[34] } else { 0 };
    // path at data[35..99] (64 bytes)
    // name at data[99..131] (32 bytes)
    let name_start = 99.min(data.len());
    let name_end = (name_start + 32).min(data.len());
    let short_id = format!("{}...", &public_key_hex[..8]);
    let advert_name = if data.len() > name_start {
        decode_mesh_name(&data[name_start..name_end], &short_id)
    } else {
        short_id
    };

    // last_advert at data[131..135]
    let last_advert = if data.len() >= 135 {
        u32::from_le_bytes([data[131], data[132], data[133], data[134]])
    } else {
        0
    };

    Ok(ParsedContact {
        public_key_hex,
        advert_name,
        last_advert,
        contact_type,
        path_len,
        flags,
        // Meshcore tracks E2E per message, not per contact.
        pkc_capable: false,
        // Meshcore's own contact format does carry lat/lon at a fixed offset
        // (see the format comment above) but wiring that up is out of scope
        // for this Meshtastic-specific backlog item.
        rssi: None,
        snr: None,
        lat: None,
        lon: None,
        arch_pubkey_hex: None,
    })
}

/// Parse RESP_CONTACT_MSG_V3 (0x10) - private message.
/// Format: SNR(1B) + reserved(2B) + pubkey_prefix(6B) + path_len(1B) + txt_type(1B) + timestamp(4B) + [sig(4B) if txt_type==2] + text
/// Returns (sender_pubkey_prefix_hex, text, snr).
pub fn parse_contact_msg_v3(data: &[u8]) -> Result<(String, String, i8)> {
    if data.len() < 15 {
        anyhow::bail!("Contact message too short: {} bytes", data.len());
    }
    let snr = data[0] as i8;
    // data[1..3] reserved
    let pubkey_prefix = hex::encode(&data[3..9]);
    // data[9] = path_len
    let txt_type = data[10];
    // data[11..15] = timestamp
    let text_start = if txt_type == 2 { 19 } else { 15 }; // skip 4-byte signature if txt_type==2
    let text = if data.len() > text_start {
        String::from_utf8_lossy(&data[text_start..]).to_string()
    } else {
        String::new()
    };
    Ok((pubkey_prefix, text, snr))
}

/// Parse RESP_CHANNEL_MSG_V3 (0x11) - channel message.
/// Format: channel_idx(1B) + path_len(1B) + txt_type(1B) + timestamp(4B) + text
/// Returns (channel_idx, text).
pub fn parse_channel_msg_v3(data: &[u8]) -> Result<(u8, String)> {
    if data.len() < 7 {
        anyhow::bail!("Channel message too short: {} bytes", data.len());
    }
    let channel_idx = data[0];
    // data[1] = path_len, data[2] = txt_type
    // data[3..7] = timestamp
    let text = if data.len() > 7 {
        String::from_utf8_lossy(&data[7..])
            .trim_end_matches('\0')
            .to_string()
    } else {
        String::new()
    };
    Ok((channel_idx, text))
}

/// Parse RESP_CONTACT_MSG (0x07) - v1 private message.
/// Format: pubkey_prefix(6B) + path_len(1B) + txt_type(1B) + timestamp(4B) + [sig(4B) if txt_type==2] + text
/// Returns (sender_pubkey_prefix_hex, text).
pub fn parse_contact_msg_v1(data: &[u8]) -> Result<(String, String)> {
    if data.len() < 12 {
        anyhow::bail!("Contact message v1 too short: {} bytes", data.len());
    }
    let pubkey_prefix = hex::encode(&data[0..6]);
    // data[6] = path_len, data[7] = txt_type
    let txt_type = data[7];
    // data[8..12] = timestamp
    let text_start = if txt_type == 2 { 16 } else { 12 };
    let text = if data.len() > text_start {
        String::from_utf8_lossy(&data[text_start..]).to_string()
    } else {
        String::new()
    };
    Ok((pubkey_prefix, text))
}

/// Parse RESP_CHANNEL_MSG (0x08) - v1 channel message.
/// Format: channel_idx(1B) + path_len(1B) + txt_type(1B) + timestamp(4B) + text
pub fn parse_channel_msg_v1(data: &[u8]) -> Result<(u8, String)> {
    if data.len() < 7 {
        anyhow::bail!("Channel message v1 too short: {} bytes", data.len());
    }
    let channel_idx = data[0];
    // data[1] = path_len, data[2] = txt_type
    // data[3..7] = timestamp
    let text = if data.len() > 7 {
        String::from_utf8_lossy(&data[7..])
            .trim_end_matches('\0')
            .to_string()
    } else {
        String::new()
    };
    Ok((channel_idx, text))
}

// ─── Raw-bytes variants for typed message detection ────────────────────

/// Parse RESP_CONTACT_MSG_V3 returning raw payload bytes (not UTF-8 lossy).
/// Returns (sender_pubkey_prefix_hex, raw_payload_bytes, snr).
pub fn parse_contact_msg_v3_raw(data: &[u8]) -> Result<(String, Vec<u8>, i8)> {
    if data.len() < 15 {
        anyhow::bail!("Contact message too short: {} bytes", data.len());
    }
    let snr = data[0] as i8;
    let pubkey_prefix = hex::encode(&data[3..9]);
    let txt_type = data[10];
    let text_start = if txt_type == 2 { 19 } else { 15 };
    let payload = if data.len() > text_start {
        data[text_start..].to_vec()
    } else {
        Vec::new()
    };
    Ok((pubkey_prefix, payload, snr))
}

/// Parse RESP_CONTACT_MSG returning raw payload bytes.
/// Returns (sender_pubkey_prefix_hex, raw_payload_bytes).
pub fn parse_contact_msg_v1_raw(data: &[u8]) -> Result<(String, Vec<u8>)> {
    if data.len() < 12 {
        anyhow::bail!("Contact message v1 too short: {} bytes", data.len());
    }
    let pubkey_prefix = hex::encode(&data[0..6]);
    let txt_type = data[7];
    let text_start = if txt_type == 2 { 16 } else { 12 };
    let payload = if data.len() > text_start {
        data[text_start..].to_vec()
    } else {
        Vec::new()
    };
    Ok((pubkey_prefix, payload))
}

/// Parse RESP_CHANNEL_MSG_V3 returning raw payload bytes.
/// Returns (channel_idx, raw_payload_bytes).
pub fn parse_channel_msg_v3_raw(data: &[u8]) -> Result<(u8, Vec<u8>)> {
    if data.len() < 7 {
        anyhow::bail!("Channel message too short: {} bytes", data.len());
    }
    let channel_idx = data[0];
    let payload = if data.len() > 7 {
        let mut p = data[7..].to_vec();
        // Strip trailing NUL bytes
        while p.last() == Some(&0) {
            p.pop();
        }
        p
    } else {
        Vec::new()
    };
    Ok((channel_idx, payload))
}

/// Parse RESP_CHANNEL_MSG returning raw payload bytes.
/// Returns (channel_idx, raw_payload_bytes).
pub fn parse_channel_msg_v1_raw(data: &[u8]) -> Result<(u8, Vec<u8>)> {
    if data.len() < 7 {
        anyhow::bail!("Channel message v1 too short: {} bytes", data.len());
    }
    let channel_idx = data[0];
    let payload = if data.len() > 7 {
        let mut p = data[7..].to_vec();
        while p.last() == Some(&0) {
            p.pop();
        }
        p
    } else {
        Vec::new()
    };
    Ok((channel_idx, payload))
}

/// Parse RESP_ERR (0x01). Returns descriptive error string.
pub fn parse_error(data: &[u8]) -> String {
    if data.is_empty() {
        return "Unknown device error".to_string();
    }
    match data[0] {
        ERR_UNSUPPORTED_CMD => "Unsupported command".to_string(),
        ERR_NOT_FOUND => "Not found".to_string(),
        ERR_TABLE_FULL => "Contact table full".to_string(),
        ERR_BAD_STATE => "Bad device state".to_string(),
        ERR_FILE_IO => "Device file I/O error".to_string(),
        ERR_ILLEGAL_ARG => "Illegal argument".to_string(),
        code => format!("Device error code 0x{:02x}", code),
    }
}

/// Check if a response code is a push notification (async event from device).
pub fn is_push_notification(code: u8) -> bool {
    code >= 0x80
}

// ─── Archipelago identity wire format ───────────────────────────────────

/// Prefix for Archipelago identity broadcasts over mesh channel.
pub const ARCHY_IDENTITY_PREFIX: &str = "ARCHY:1:";

/// Encode an Archipelago identity announcement for channel broadcast.
/// Compact format: `ARCHY:2:{ed25519_pubkey_hex}:{x25519_pubkey_hex}`
/// DID is omitted to fit within 160-byte LoRa limit — receiver reconstructs did:key from ed25519 pubkey.
/// Total: 8 + 64 + 1 + 64 = 137 bytes (fits in 160).
pub fn encode_identity_broadcast(
    _did: &str,
    ed_pubkey_hex: &str,
    x25519_pubkey_hex: &str,
) -> String {
    format!("ARCHY:2:{}:{}", ed_pubkey_hex, x25519_pubkey_hex)
}

/// Try to parse an Archipelago identity from a received channel message.
/// Returns (did, ed25519_pubkey_hex, x25519_pubkey_hex) if valid.
///
/// Supports two formats:
/// - v2 (compact): `ARCHY:2:{ed25519_hex_64}:{x25519_hex_64}` — DID reconstructed from ed25519
/// - v1 (legacy):  `ARCHY:1:{did}:{ed25519_hex_64}:{x25519_hex_64}`
pub fn parse_identity_broadcast(msg: &str) -> Option<(String, String, String)> {
    // Try v2 compact format first
    if let Some(rest) = msg.strip_prefix("ARCHY:2:") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        let ed_pubkey = parts[0];
        let x25519_pubkey = parts[1];
        if ed_pubkey.len() != 64 || x25519_pubkey.len() != 64 {
            return None;
        }
        if !ed_pubkey.chars().all(|c| c.is_ascii_hexdigit())
            || !x25519_pubkey.chars().all(|c| c.is_ascii_hexdigit())
        {
            return None;
        }
        // Reconstruct DID from ed25519 pubkey
        let did = crate::identity::did_key_from_pubkey_hex(ed_pubkey).ok()?;
        return Some((did, ed_pubkey.to_string(), x25519_pubkey.to_string()));
    }

    // Try v1 legacy format
    let rest = msg.strip_prefix(ARCHY_IDENTITY_PREFIX)?;
    let last_colon = rest.rfind(':')?;
    let x25519_pubkey = &rest[last_colon + 1..];
    if x25519_pubkey.len() != 64 || !x25519_pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let before_x25519 = &rest[..last_colon];
    let second_last_colon = before_x25519.rfind(':')?;
    let ed_pubkey = &before_x25519[second_last_colon + 1..];
    if ed_pubkey.len() != 64 || !ed_pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let did = &before_x25519[..second_last_colon];
    if !did.starts_with("did:key:z") {
        return None;
    }
    Some((
        did.to_string(),
        ed_pubkey.to_string(),
        x25519_pubkey.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_frame() {
        let frame = encode_frame(&[CMD_DEVICE_QUERY, PROTOCOL_VERSION]);
        assert_eq!(frame[0], OUTBOUND_MARKER);
        assert_eq!(u16::from_le_bytes([frame[1], frame[2]]), 2);
        assert_eq!(frame[3], CMD_DEVICE_QUERY);
        assert_eq!(frame[4], PROTOCOL_VERSION);
    }

    #[test]
    fn test_build_set_radio_params_wire_layout() {
        // Portugal preset: 869.618 MHz, 62.5 kHz BW, SF 8, CR 8.
        // freq field = MHz*1000 = 869618; bw field = kHz*1000 = 62500.
        let frame = build_set_radio_params(869_618, 62_500, 8, 8);
        assert_eq!(frame[0], OUTBOUND_MARKER);
        // payload length = 1 (cmd) + 4 (freq) + 4 (bw) + 1 (sf) + 1 (cr) = 11
        assert_eq!(u16::from_le_bytes([frame[1], frame[2]]), 11);
        let data = &frame[3..];
        assert_eq!(data[0], CMD_SET_RADIO_PARAMS);
        assert_eq!(
            u32::from_le_bytes([data[1], data[2], data[3], data[4]]),
            869_618
        );
        assert_eq!(
            u32::from_le_bytes([data[5], data[6], data[7], data[8]]),
            62_500
        );
        assert_eq!(data[9], 8); // sf
        assert_eq!(data[10], 8); // cr
        assert_eq!(data.len(), 11);
    }

    #[test]
    fn test_decode_frame_complete() -> Result<()> {
        // Simulate an inbound frame: < + len(2) + [RESP_OK]
        let buf = vec![INBOUND_MARKER, 0x01, 0x00, RESP_OK];
        let frame =
            decode_frame(&buf).ok_or_else(|| anyhow::anyhow!("failed to parse complete frame"))?;
        assert_eq!(frame.code, RESP_OK);
        assert!(frame.data.is_empty());
        assert_eq!(frame.bytes_consumed, 4);
        Ok(())
    }

    #[test]
    fn test_decode_frame_with_data() -> Result<()> {
        // < + len(5) + [RESP_SELF_INFO, 0x01, 0x02, 0x03, 0x04]
        let buf = vec![
            INBOUND_MARKER,
            0x05,
            0x00,
            RESP_SELF_INFO,
            0x01,
            0x02,
            0x03,
            0x04,
        ];
        let frame =
            decode_frame(&buf).ok_or_else(|| anyhow::anyhow!("failed to parse frame with data"))?;
        assert_eq!(frame.code, RESP_SELF_INFO);
        assert_eq!(frame.data, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(frame.bytes_consumed, 8);
        Ok(())
    }

    #[test]
    fn test_decode_frame_incomplete() {
        let buf = vec![INBOUND_MARKER, 0x05, 0x00, RESP_OK]; // says 5 bytes but only 1
        assert!(decode_frame(&buf).is_none());
    }

    #[test]
    fn test_decode_frame_no_marker() {
        let buf = vec![0xFF, 0x01, 0x00, RESP_OK];
        assert!(decode_frame(&buf).is_none());
    }

    #[test]
    fn test_decode_frame_skips_garbage() -> Result<()> {
        // Garbage bytes before the actual frame
        let buf = vec![0xFF, 0xAA, INBOUND_MARKER, 0x01, 0x00, RESP_OK];
        let frame = decode_frame(&buf)
            .ok_or_else(|| anyhow::anyhow!("failed to skip garbage and parse frame"))?;
        assert_eq!(frame.code, RESP_OK);
        assert_eq!(frame.bytes_consumed, 6); // 2 garbage + 4 frame
        Ok(())
    }

    #[test]
    fn test_build_device_query() {
        let frame = build_device_query();
        assert_eq!(frame[0], OUTBOUND_MARKER);
        assert_eq!(frame[3], CMD_DEVICE_QUERY);
        assert_eq!(frame[4], PROTOCOL_VERSION);
    }

    #[test]
    fn test_build_app_start() -> Result<()> {
        // Frame layout: [0: '>'][1-2: len LE][3: CMD][4: VERSION][5..: padded name]
        let frame = build_app_start("Archipelago");
        assert_eq!(frame[3], CMD_APP_START);
        assert_eq!(frame[4], PROTOCOL_VERSION);
        let name = &frame[5..];
        assert_eq!(
            std::str::from_utf8(name)
                .map_err(|e| anyhow::anyhow!("invalid UTF-8 in app name: {}", e))?,
            "Archipelago"
        );
        Ok(())
    }

    #[test]
    fn test_build_set_device_time() {
        let ts: u64 = 1710600000;
        let frame = build_set_device_time(ts);
        assert_eq!(frame[3], CMD_SET_DEVICE_TIME);
        let time_bytes = &frame[4..8];
        assert_eq!(
            u32::from_le_bytes([time_bytes[0], time_bytes[1], time_bytes[2], time_bytes[3]]),
            ts as u32
        );
    }

    #[test]
    fn test_build_send_text() -> Result<()> {
        let dest: [u8; 6] = [0x00, 0x00, 0x00, 0x2A, 0x00, 0x00];
        let frame = build_send_text(&dest, b"hello")?;
        assert_eq!(frame[3], CMD_SEND_TXT_MSG);
        Ok(())
    }

    #[test]
    fn test_build_send_text_too_large() {
        let dest: [u8; 6] = [0x00; 6];
        let big = vec![0u8; MAX_MESSAGE_LEN + 1];
        assert!(build_send_text(&dest, &big).is_err());
    }

    #[test]
    fn test_build_send_channel_text() -> Result<()> {
        let frame = build_send_channel_text(2, b"test")?;
        // Frame: [marker][len_lo][len_hi][cmd][txt_type][channel][ts(4)][text]
        assert_eq!(frame[3], CMD_SEND_CHANNEL_TXT_MSG);
        assert_eq!(frame[4], 0); // txt_type
        assert_eq!(frame[5], 2); // channel idx
                                 // frame[6..10] = timestamp, non-deterministic
        assert_eq!(&frame[10..], b"test");
        Ok(())
    }

    #[test]
    fn test_identity_broadcast_roundtrip() -> Result<()> {
        // The v2 encoding drops the DID and the decoder reconstructs it
        // deterministically from the ed25519 pubkey, so the roundtripped
        // DID won't equal an arbitrary input DID. Derive what the decoder
        // will produce and assert against that.
        let ed_pub = "a".repeat(64);
        let x25519_pub = "b".repeat(64);
        let expected_did = crate::identity::did_key_from_pubkey_hex(&ed_pub)
            .map_err(|e| anyhow::anyhow!("derive did: {}", e))?;

        let encoded = encode_identity_broadcast(&expected_did, &ed_pub, &x25519_pub);

        let (parsed_did, parsed_ed, parsed_x) = parse_identity_broadcast(&encoded)
            .ok_or_else(|| anyhow::anyhow!("failed to parse identity broadcast"))?;
        assert_eq!(parsed_did, expected_did);
        assert_eq!(parsed_ed, ed_pub);
        assert_eq!(parsed_x, x25519_pub);
        Ok(())
    }

    #[test]
    fn test_identity_broadcast_invalid() {
        assert!(parse_identity_broadcast("not an identity").is_none());
        assert!(parse_identity_broadcast("ARCHY:1:bad").is_none());
        assert!(parse_identity_broadcast("ARCHY:1:did:key:z123:short:short").is_none());
    }

    #[test]
    fn test_parse_error_codes() {
        assert_eq!(parse_error(&[ERR_NOT_FOUND]), "Not found");
        assert_eq!(parse_error(&[ERR_TABLE_FULL]), "Contact table full");
        assert_eq!(parse_error(&[]), "Unknown device error");
        assert!(parse_error(&[0xFF]).contains("0xff"));
    }

    #[test]
    fn test_is_push_notification() {
        assert!(is_push_notification(PUSH_NEW_CONTACT));
        assert!(is_push_notification(PUSH_ACK));
        assert!(is_push_notification(0x80));
        assert!(!is_push_notification(RESP_OK));
        assert!(!is_push_notification(RESP_DEVICE_INFO));
    }

    #[test]
    fn test_parse_self_info() -> Result<()> {
        let mut data = vec![0x2A, 0x00, 0x00, 0x00]; // node_id = 42
        data.extend_from_slice(b"TestNode\0");
        let (id, name) = parse_self_info(&data)?;
        assert_eq!(id, 42);
        assert_eq!(name, "TestNode");
        Ok(())
    }

    #[test]
    fn test_parse_self_info_too_short() {
        assert!(parse_self_info(&[0x01, 0x02]).is_err());
    }

    #[test]
    fn current_companion_binary_metadata_is_not_a_name_or_version() {
        let mut info = vec![0u8; 81];
        info[0] = 13;
        info[1] = 150;
        info[19..28].copy_from_slice(b"Heltec V3");
        info[59..74].copy_from_slice(b"v1.17.1-d929643");
        assert_eq!(
            parse_device_info(&info).unwrap(),
            ("v1.17.1-d929643".into(), 300)
        );
        assert!(parse_device_info(&info[..78]).is_err());
        let mut own = vec![0u8; 57];
        own[0..3].copy_from_slice(&[1, 22, 22]);
        own[3..7].copy_from_slice(&42u32.to_le_bytes());
        own[47..51].copy_from_slice(&869618u32.to_le_bytes());
        own.extend_from_slice(b"My radio");
        assert_eq!(parse_self_info(&own).unwrap(), (42, "My radio".into()));
    }
}
