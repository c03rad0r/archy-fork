// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Steganographic encoding for mesh messages.
//!
//! Transforms typed message envelopes into formats that resemble innocuous
//! sensor data on the wire. Provides plausible deniability — traffic analysis
//! sees weather readings or industrial sensor data, not Bitcoin transactions.
//!
//! Wire format:
//! - Normal: `[0x02] [CBOR envelope]` (existing)
//! - Stego:  `[0xAA] [mode: 1 byte] [stego-encoded data]`
//!
//! The 0xAA prefix distinguishes steganographic frames from typed (0x02) and
//! plain text (0x00) messages. Both sender and receiver must use the same mode.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Wire prefix for steganographic messages.
pub const STEGO_MARKER: u8 = 0xAA;

/// Steganography mode — how real payload bytes are disguised on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SteganographyMode {
    /// No steganography — standard 0x02 typed envelope.
    #[default]
    Normal,
    /// Payload disguised as weather station telemetry.
    /// Format: repeating 8-byte "readings" (temp, humidity, pressure, wind, flags).
    WeatherStation,
    /// Payload disguised as industrial sensor network data.
    /// Format: repeating 6-byte "samples" (voltage, current, vibration, status).
    SensorNetwork,
}

impl SteganographyMode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Normal),
            1 => Some(Self::WeatherStation),
            2 => Some(Self::SensorNetwork),
            _ => None,
        }
    }
}

// ─── Weather Station Encoding ──────────────────────────────────────────
//
// Each 8-byte "reading" encodes 5 bytes of real payload data:
//   [temp_hi: u8] [temp_lo: u8] [humidity: u8] [pressure_hi: u8] [pressure_lo: u8]
//   [wind_speed: u8] [wind_dir: u8] [flags: u8]
//
// Real data bytes map as:
//   byte0 → temp_hi (offset by 200 to look like -50.0°C to +5.5°C range)
//   byte1 → humidity (modulo 100)
//   byte2 → pressure_hi (offset by 900 for 900-1155 hPa range)
//   byte3 → wind_speed (modulo 60 for 0-59 m/s)
//   byte4 → flags (lower 5 bits = data, upper 3 bits = plausible status flags)
//
// temp_lo, pressure_lo, wind_dir are derived (not payload data) for realism.
// Overhead: 8 bytes per 5 payload bytes = 60% efficiency.

const WEATHER_REAL_BYTES_PER_BLOCK: usize = 5;
const WEATHER_WIRE_BYTES_PER_BLOCK: usize = 8;

fn encode_weather_block(data: &[u8]) -> [u8; WEATHER_WIRE_BYTES_PER_BLOCK] {
    let mut block = [0u8; 8];
    let b0 = *data.first().unwrap_or(&0);
    let b1 = *data.get(1).unwrap_or(&0);
    let b2 = *data.get(2).unwrap_or(&0);
    let b3 = *data.get(3).unwrap_or(&0);
    let b4 = *data.get(4).unwrap_or(&0);

    // temp: b0 mapped to plausible range, fractional derived from b1
    block[0] = b0.wrapping_add(200); // temp_hi — wraps around, decoded by subtracting 200
    block[1] = b1 ^ 0x55; // temp_lo — XOR mask, recoverable
                          // humidity: b1 stored directly (0-255 maps to 0-100% with modular interpretation)
    block[2] = b1;
    // pressure: b2 offset into 900-1155 range
    block[3] = b2;
    block[4] = b3 ^ 0x33; // pressure_lo — XOR mask
                          // wind: b3 modular
    block[5] = b3;
    // wind direction: derived from b4 (0-359 degrees as single byte = 0-255 → *1.41)
    block[6] = b4 ^ 0xAA; // XOR mask
                          // flags: b4 with upper bits set for realism (battery OK, GPS lock, etc.)
    block[7] = (b4 & 0x1F) | 0xC0; // upper 2 bits always set

    block
}

fn decode_weather_block(
    block: &[u8; WEATHER_WIRE_BYTES_PER_BLOCK],
) -> [u8; WEATHER_REAL_BYTES_PER_BLOCK] {
    let mut data = [0u8; 5];
    data[0] = block[0].wrapping_sub(200);
    data[1] = block[2]; // humidity field stores b1 directly
    data[2] = block[3]; // pressure_hi stores b2 directly
    data[3] = block[5]; // wind_speed stores b3 directly
    data[4] = block[6] ^ 0xAA; // wind_dir XOR back
    data
}

// ─── Sensor Network Encoding ───────────────────────────────────────────
//
// Each 6-byte "sample" encodes 4 bytes of real payload data:
//   [voltage_hi: u8] [voltage_lo: u8] [current: u8]
//   [vibration: u8] [phase: u8] [status: u8]
//
// Real data bytes map as:
//   byte0 → voltage_hi
//   byte1 → current
//   byte2 → vibration
//   byte3 → status (lower 4 bits = data, upper 4 = plausible status)
//
// voltage_lo and phase are derived for realism.
// Overhead: 6 bytes per 4 payload bytes = 67% efficiency.

const SENSOR_REAL_BYTES_PER_BLOCK: usize = 4;
const SENSOR_WIRE_BYTES_PER_BLOCK: usize = 6;

fn encode_sensor_block(data: &[u8]) -> [u8; SENSOR_WIRE_BYTES_PER_BLOCK] {
    let mut block = [0u8; 6];
    let b0 = *data.first().unwrap_or(&0);
    let b1 = *data.get(1).unwrap_or(&0);
    let b2 = *data.get(2).unwrap_or(&0);
    let b3 = *data.get(3).unwrap_or(&0);

    block[0] = b0; // voltage_hi
    block[1] = b0 ^ b1; // voltage_lo (derived, recoverable)
    block[2] = b1; // current
    block[3] = b2; // vibration
    block[4] = b2.wrapping_add(b3); // phase (derived)
    block[5] = (b3 & 0x0F) | 0x80; // status: upper nibble = "operational"

    block
}

fn decode_sensor_block(
    block: &[u8; SENSOR_WIRE_BYTES_PER_BLOCK],
) -> [u8; SENSOR_REAL_BYTES_PER_BLOCK] {
    let mut data = [0u8; 4];
    data[0] = block[0]; // voltage_hi = b0
    data[1] = block[2]; // current = b1
    data[2] = block[3]; // vibration = b2
    data[3] = (block[5] & 0x0F) | (block[4].wrapping_sub(block[3]) & 0xF0);
    // Recover b3: lower 4 bits from status, but we only stored lower 4.
    // Full b3 recovery: block[4] = b2 + b3, so b3 = block[4] - block[3]
    data[3] = block[4].wrapping_sub(block[3]);
    data
}

// ─── Public API ────────────────────────────────────────────────────────

/// Encode raw payload bytes using steganographic mode.
/// Returns: `[0xAA] [mode_byte] [length_hi] [length_lo] [encoded_blocks...]`
///
/// The length field stores the original payload size (up to 65535 bytes)
/// so the decoder knows how many real bytes to extract.
pub fn encode(mode: SteganographyMode, payload: &[u8]) -> Result<Vec<u8>> {
    if mode == SteganographyMode::Normal {
        anyhow::bail!("Cannot steganographically encode in Normal mode");
    }
    if payload.len() > 0xFFFF {
        anyhow::bail!("Payload too large for steganographic encoding");
    }

    let len = payload.len() as u16;
    let mut output = vec![
        STEGO_MARKER,
        mode as u8,
        (len >> 8) as u8,
        (len & 0xFF) as u8,
    ];

    match mode {
        SteganographyMode::WeatherStation => {
            for chunk in payload.chunks(WEATHER_REAL_BYTES_PER_BLOCK) {
                // Pad short final chunk with zeros
                let mut padded = [0u8; WEATHER_REAL_BYTES_PER_BLOCK];
                padded[..chunk.len()].copy_from_slice(chunk);
                output.extend_from_slice(&encode_weather_block(&padded));
            }
        }
        SteganographyMode::SensorNetwork => {
            for chunk in payload.chunks(SENSOR_REAL_BYTES_PER_BLOCK) {
                let mut padded = [0u8; SENSOR_REAL_BYTES_PER_BLOCK];
                padded[..chunk.len()].copy_from_slice(chunk);
                output.extend_from_slice(&encode_sensor_block(&padded));
            }
        }
        SteganographyMode::Normal => unreachable!(),
    }

    Ok(output)
}

/// Decode a steganographic frame back to raw payload bytes.
/// Input must start with `0xAA`.
pub fn decode(data: &[u8]) -> Result<(SteganographyMode, Vec<u8>)> {
    if data.len() < 4 {
        anyhow::bail!("Stego frame too short: {} bytes", data.len());
    }
    if data[0] != STEGO_MARKER {
        anyhow::bail!("Not a stego frame (expected 0xAA, got 0x{:02x})", data[0]);
    }

    let mode = SteganographyMode::from_u8(data[1])
        .ok_or_else(|| anyhow::anyhow!("Unknown stego mode: 0x{:02x}", data[1]))?;
    let original_len = ((data[2] as usize) << 8) | (data[3] as usize);
    let encoded_data = &data[4..];

    let mut payload = Vec::with_capacity(original_len);

    match mode {
        SteganographyMode::WeatherStation => {
            for block_bytes in encoded_data.chunks(WEATHER_WIRE_BYTES_PER_BLOCK) {
                if block_bytes.len() < WEATHER_WIRE_BYTES_PER_BLOCK {
                    break;
                }
                let block: [u8; WEATHER_WIRE_BYTES_PER_BLOCK] = block_bytes
                    .try_into()
                    .context("Invalid weather block size")?;
                let decoded = decode_weather_block(&block);
                payload.extend_from_slice(&decoded);
            }
        }
        SteganographyMode::SensorNetwork => {
            for block_bytes in encoded_data.chunks(SENSOR_WIRE_BYTES_PER_BLOCK) {
                if block_bytes.len() < SENSOR_WIRE_BYTES_PER_BLOCK {
                    break;
                }
                let block: [u8; SENSOR_WIRE_BYTES_PER_BLOCK] = block_bytes
                    .try_into()
                    .context("Invalid sensor block size")?;
                let decoded = decode_sensor_block(&block);
                payload.extend_from_slice(&decoded);
            }
        }
        SteganographyMode::Normal => {
            anyhow::bail!("Normal mode cannot appear in stego frame");
        }
    }

    // Truncate to original length (removes padding from last block)
    payload.truncate(original_len);
    Ok((mode, payload))
}

/// Encode a typed envelope wire bytes using steganography.
/// Input: standard wire bytes starting with 0x02 (TYPED_MESSAGE_MARKER).
/// Output: stego wire bytes starting with 0xAA.
pub fn encode_typed_wire(mode: SteganographyMode, typed_wire: &[u8]) -> Result<Vec<u8>> {
    if typed_wire.is_empty() || typed_wire[0] != super::message_types::TYPED_MESSAGE_MARKER {
        anyhow::bail!("Input is not a typed message (expected 0x02 prefix)");
    }
    // Encode the entire typed wire frame (including the 0x02 marker) as payload
    encode(mode, typed_wire)
}

/// Decode a stego frame back to typed envelope wire bytes.
/// Returns the original bytes with 0x02 prefix restored.
pub fn decode_typed_wire(stego_data: &[u8]) -> Result<Vec<u8>> {
    let (_mode, payload) = decode(stego_data)?;
    if payload.is_empty() || payload[0] != super::message_types::TYPED_MESSAGE_MARKER {
        anyhow::bail!("Decoded stego payload is not a typed message");
    }
    Ok(payload)
}

/// Calculate the wire overhead for a given mode and payload size.
pub fn wire_size(mode: SteganographyMode, payload_len: usize) -> usize {
    let header = 4; // 0xAA + mode + len_hi + len_lo
    match mode {
        SteganographyMode::Normal => payload_len,
        SteganographyMode::WeatherStation => {
            let blocks = payload_len.div_ceil(WEATHER_REAL_BYTES_PER_BLOCK);
            header + blocks * WEATHER_WIRE_BYTES_PER_BLOCK
        }
        SteganographyMode::SensorNetwork => {
            let blocks = payload_len.div_ceil(SENSOR_REAL_BYTES_PER_BLOCK);
            header + blocks * SENSOR_WIRE_BYTES_PER_BLOCK
        }
    }
}

/// Max real payload bytes that fit in a single 160-byte LoRa frame after stego.
pub fn max_payload_per_frame(mode: SteganographyMode) -> usize {
    let frame_limit = 160usize;
    let header = 4;
    let available = frame_limit.saturating_sub(header);
    match mode {
        SteganographyMode::Normal => frame_limit - 1, // minus 0x02 marker
        SteganographyMode::WeatherStation => {
            let blocks = available / WEATHER_WIRE_BYTES_PER_BLOCK;
            blocks * WEATHER_REAL_BYTES_PER_BLOCK
        }
        SteganographyMode::SensorNetwork => {
            let blocks = available / SENSOR_WIRE_BYTES_PER_BLOCK;
            blocks * SENSOR_REAL_BYTES_PER_BLOCK
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weather_roundtrip() {
        let original = vec![0x42, 0xFF, 0x00, 0xAB, 0x13];
        let encoded = encode(SteganographyMode::WeatherStation, &original).unwrap();
        assert_eq!(encoded[0], STEGO_MARKER);
        assert_eq!(encoded[1], SteganographyMode::WeatherStation as u8);
        let (mode, decoded) = decode(&encoded).unwrap();
        assert_eq!(mode, SteganographyMode::WeatherStation);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_sensor_roundtrip() {
        let original = vec![0x42, 0xFF, 0x00, 0xAB];
        let encoded = encode(SteganographyMode::SensorNetwork, &original).unwrap();
        assert_eq!(encoded[0], STEGO_MARKER);
        let (mode, decoded) = decode(&encoded).unwrap();
        assert_eq!(mode, SteganographyMode::SensorNetwork);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_weather_multi_block() {
        // 12 bytes = 3 weather blocks (5+5+2 with padding)
        let original: Vec<u8> = (0..12).collect();
        let encoded = encode(SteganographyMode::WeatherStation, &original).unwrap();
        let (_, decoded) = decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_sensor_multi_block() {
        // 10 bytes = 3 sensor blocks (4+4+2 with padding)
        let original: Vec<u8> = (0..10).collect();
        let encoded = encode(SteganographyMode::SensorNetwork, &original).unwrap();
        let (_, decoded) = decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_all_byte_values_weather() {
        let original: Vec<u8> = (0..=255).collect();
        let encoded = encode(SteganographyMode::WeatherStation, &original).unwrap();
        let (_, decoded) = decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_all_byte_values_sensor() {
        let original: Vec<u8> = (0..=255).collect();
        let encoded = encode(SteganographyMode::SensorNetwork, &original).unwrap();
        let (_, decoded) = decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_empty_payload() {
        let encoded = encode(SteganographyMode::WeatherStation, &[]).unwrap();
        let (_, decoded) = decode(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_wire_size_calculation() {
        // 5 bytes payload = 1 weather block = 4 header + 8 = 12
        assert_eq!(wire_size(SteganographyMode::WeatherStation, 5), 12);
        // 4 bytes payload = 1 sensor block = 4 header + 6 = 10
        assert_eq!(wire_size(SteganographyMode::SensorNetwork, 4), 10);
    }

    #[test]
    fn test_max_payload_per_frame() {
        let weather_max = max_payload_per_frame(SteganographyMode::WeatherStation);
        let sensor_max = max_payload_per_frame(SteganographyMode::SensorNetwork);
        // Verify the encoded output fits in 160 bytes
        let test_data = vec![0x42; weather_max];
        let encoded = encode(SteganographyMode::WeatherStation, &test_data).unwrap();
        assert!(
            encoded.len() <= 160,
            "Weather stego {} > 160",
            encoded.len()
        );

        let test_data = vec![0x42; sensor_max];
        let encoded = encode(SteganographyMode::SensorNetwork, &test_data).unwrap();
        assert!(encoded.len() <= 160, "Sensor stego {} > 160", encoded.len());
    }

    #[test]
    fn test_normal_mode_rejects() {
        assert!(encode(SteganographyMode::Normal, &[1, 2, 3]).is_err());
    }

    #[test]
    fn test_typed_wire_roundtrip() {
        // Simulate a typed message wire frame
        let mut typed_wire = vec![0x02]; // TYPED_MESSAGE_MARKER
        typed_wire.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05]);
        let stego = encode_typed_wire(SteganographyMode::WeatherStation, &typed_wire).unwrap();
        let recovered = decode_typed_wire(&stego).unwrap();
        assert_eq!(recovered, typed_wire);
    }
}
