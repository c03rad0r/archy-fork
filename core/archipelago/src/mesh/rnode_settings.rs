//! Persisted RNode LoRa RF settings — the operator-editable half of the
//! Reticulum transport (.126 LoRa settings panel).
//!
//! The reticulum sidecar (reticulum-daemon) writes the RNS config from its
//! CLI args at every spawn; before this module those args were never passed,
//! so every node ran the sidecar's argparse defaults and nothing was
//! operator-editable. These settings persist at
//! `<data_dir>/rnode-rf-settings.json`, feed `daemon_command` as explicit
//! args, and the panel confirms application via the sidecar's `radio_state`
//! read-back (the radio-confirmed `r_*` values, not the requested ones).
//!
//! An absent file yields [`RNodeRfSettings::default`], which matches the
//! sidecar's historical argparse defaults exactly — deploying this changes
//! nothing until the operator edits something.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const SETTINGS_FILE: &str = "rnode-rf-settings.json";

/// Validation bounds mirror RNS `RNodeInterface.py` (`validate_firmware` /
/// the constructor checks) — NOT guessed: frequency 137–1020 MHz, sf 5–12,
/// cr 5–8, txpower 0–22 dBm, airtime locks 0–100 %.
const FREQ_MIN_HZ: u64 = 137_000_000;
const FREQ_MAX_HZ: u64 = 1_020_000_000;
/// The discrete bandwidths RNode firmware accepts (Hz).
const VALID_BANDWIDTHS: &[u64] = &[
    7_800, 10_400, 15_600, 20_800, 31_250, 41_700, 62_500, 125_000, 250_000, 500_000,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RNodeRfSettings {
    /// Interface on/off. `false` keeps the daemon from opening the radio at
    /// all (the mesh service skips the serial transport).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Serial device override (e.g. `/dev/ttyACM0`). `None` = auto-detect,
    /// which is what every node did before this existed.
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default = "default_frequency")]
    pub frequency: u64,
    #[serde(default = "default_bandwidth")]
    pub bandwidth: u64,
    #[serde(default = "default_spreading_factor")]
    pub spreading_factor: u8,
    #[serde(default = "default_coding_rate")]
    pub coding_rate: u8,
    #[serde(default = "default_txpower")]
    pub txpower: u8,
    /// Short-window airtime duty-cycle lock, percent (EU868: 25). `None` =
    /// no software lock (RNS default).
    #[serde(default)]
    pub airtime_limit_short: Option<f64>,
    /// Long-window airtime duty-cycle lock, percent (EU868: 10).
    #[serde(default)]
    pub airtime_limit_long: Option<f64>,
}

fn default_true() -> bool {
    true
}
fn default_frequency() -> u64 {
    869_525_000
}
fn default_bandwidth() -> u64 {
    125_000
}
fn default_spreading_factor() -> u8 {
    8
}
fn default_coding_rate() -> u8 {
    5
}
fn default_txpower() -> u8 {
    17
}

impl Default for RNodeRfSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            port: None,
            frequency: default_frequency(),
            bandwidth: default_bandwidth(),
            spreading_factor: default_spreading_factor(),
            coding_rate: default_coding_rate(),
            txpower: default_txpower(),
            airtime_limit_short: None,
            airtime_limit_long: None,
        }
    }
}

impl RNodeRfSettings {
    pub fn validate(&self) -> Result<()> {
        if !(FREQ_MIN_HZ..=FREQ_MAX_HZ).contains(&self.frequency) {
            bail!(
                "frequency {} Hz is outside the RNode range ({}–{} Hz)",
                self.frequency,
                FREQ_MIN_HZ,
                FREQ_MAX_HZ
            );
        }
        if !VALID_BANDWIDTHS.contains(&self.bandwidth) {
            bail!(
                "bandwidth {} Hz is not an RNode bandwidth (valid: {:?})",
                self.bandwidth,
                VALID_BANDWIDTHS
            );
        }
        if !(5..=12).contains(&self.spreading_factor) {
            bail!("spreading factor {} is outside 5–12", self.spreading_factor);
        }
        if !(5..=8).contains(&self.coding_rate) {
            bail!("coding rate {} is outside 5–8", self.coding_rate);
        }
        if self.txpower > 22 {
            bail!(
                "tx power {} dBm is above the 22 dBm RNode maximum",
                self.txpower
            );
        }
        for (label, v) in [
            ("airtime_limit_short", self.airtime_limit_short),
            ("airtime_limit_long", self.airtime_limit_long),
        ] {
            if let Some(pct) = v {
                if !(0.0..=100.0).contains(&pct) || !pct.is_finite() {
                    bail!("{label} {pct} is not a percentage (0–100)");
                }
            }
        }
        if let Some(port) = &self.port {
            // Same shape the flasher accepts: an absolute device node. Keeps
            // shell-metacharacter garbage out of the sidecar's argv.
            if !port.starts_with("/dev/")
                || port.chars().any(|c| {
                    !(c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.')
                })
            {
                bail!("port must be an absolute /dev device path");
            }
        }
        Ok(())
    }

    pub async fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(SETTINGS_FILE);
        match tokio::fs::read_to_string(&path).await {
            Ok(raw) => match serde_json::from_str::<Self>(&raw) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "rnode-rf-settings.json unparseable — using defaults");
                    Self::default()
                }
            },
            // First run after the update: no settings file yet. ADOPT the
            // node's existing effective RF config rather than imposing
            // defaults — the operator's standing requirement is that the
            // update changes NO device's applied settings. For archy-managed
            // radios the sidecar config equals our defaults anyway; this
            // covers any node whose RNS config diverged (hand edits,
            // hand-run rnsd).
            Err(_) => {
                let adopted = Self::adopt_existing_rns_config().await;
                if let Some(adopted) = adopted {
                    tracing::info!(
                        settings = ?adopted,
                        "adopted existing RNS RNode config as initial RF settings"
                    );
                    if let Err(e) = adopted.save(data_dir).await {
                        tracing::warn!(error = %e, "could not persist adopted RF settings");
                    }
                    adopted
                } else {
                    Self::default()
                }
            }
        }
    }

    /// Parse the RNodeInterface section out of an existing RNS config file
    /// (the sidecar's `~/.archy-reticulum/config`, else a hand-run rnsd's
    /// `~/.reticulum/config`). Returns `None` when neither exists or no
    /// RNodeInterface section is found. Unparseable/absent fields keep the
    /// default (which equals the sidecar's historical argparse default).
    async fn adopt_existing_rns_config() -> Option<Self> {
        let home = std::env::var("HOME").ok()?;
        for candidate in [
            format!("{home}/.archy-reticulum/config"),
            format!("{home}/.reticulum/config"),
        ] {
            let Ok(raw) = tokio::fs::read_to_string(&candidate).await else {
                continue;
            };
            if let Some(s) = Self::parse_rnode_section(&raw) {
                return Some(s);
            }
        }
        None
    }

    /// Extract RNode parameters from RNS config text. Scoped to the block
    /// after a `type = RNodeInterface` line so TCP interface options can
    /// never bleed in; stops at the next `[[...]]` section header.
    fn parse_rnode_section(raw: &str) -> Option<Self> {
        let mut in_rnode = false;
        let mut seen_any = false;
        let mut s = Self::default();
        for line in raw.lines() {
            let line = line.trim();
            if line.starts_with("[[") {
                if in_rnode {
                    break; // next interface section — RNode block ended
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key == "type" {
                in_rnode = value == "RNodeInterface";
                continue;
            }
            if !in_rnode {
                continue;
            }
            seen_any = true;
            match key {
                "enabled" | "interface_enabled" => {
                    s.enabled = matches!(value.to_ascii_lowercase().as_str(), "yes" | "true" | "on")
                }
                "port" => s.port = Some(value.to_string()),
                "frequency" => s.frequency = value.parse().unwrap_or(s.frequency),
                "bandwidth" => s.bandwidth = value.parse().unwrap_or(s.bandwidth),
                "txpower" => s.txpower = value.parse().unwrap_or(s.txpower),
                "spreadingfactor" => {
                    s.spreading_factor = value.parse().unwrap_or(s.spreading_factor)
                }
                "codingrate" => s.coding_rate = value.parse().unwrap_or(s.coding_rate),
                "airtime_limit_short" => s.airtime_limit_short = value.parse().ok(),
                "airtime_limit_long" => s.airtime_limit_long = value.parse().ok(),
                _ => {}
            }
        }
        (in_rnode || seen_any).then_some(s)
    }

    pub async fn save(&self, data_dir: &Path) -> Result<()> {
        self.validate()?;
        let path = data_dir.join(SETTINGS_FILE);
        let tmp = path.with_extension("json.tmp");
        let raw = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&tmp, raw).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_sidecar_argparse_defaults() {
        // reticulum_daemon.py: --frequency 869525000 --bandwidth 125000
        // --txpower 17 --spreadingfactor 8 --codingrate 5, no airtime locks.
        let d = RNodeRfSettings::default();
        assert_eq!(d.frequency, 869_525_000);
        assert_eq!(d.bandwidth, 125_000);
        assert_eq!(d.txpower, 17);
        assert_eq!(d.spreading_factor, 8);
        assert_eq!(d.coding_rate, 5);
        assert!(d.airtime_limit_short.is_none() && d.airtime_limit_long.is_none());
        assert!(d.enabled && d.port.is_none());
        d.validate().unwrap();
    }

    #[test]
    fn operator_portugal_config_validates() {
        // The operator's real device config (2026-08-06).
        let s = RNodeRfSettings {
            enabled: true,
            port: Some("/dev/ttyACM0".into()),
            frequency: 869_462_500,
            bandwidth: 125_000,
            spreading_factor: 8,
            coding_rate: 5,
            txpower: 14,
            airtime_limit_short: Some(25.0),
            airtime_limit_long: Some(10.0),
        };
        s.validate().unwrap();
    }

    #[test]
    fn adoption_preserves_the_operator_portugal_config_exactly() {
        // The operator's literal RNS config (2026-08-06). The update must
        // adopt these values verbatim — changing a node's applied RF
        // settings is forbidden.
        let raw = "\
[reticulum]
  enable_transport = yes

[interfaces]
  [[RNode LoRa Portugal]]
    type = RNodeInterface
    interface_enabled = true
    port = /dev/ttyACM0
    frequency = 869462500
    bandwidth = 125000
    spreadingfactor = 8
    codingrate = 5
    txpower = 14
    airtime_limit_short = 25
    airtime_limit_long = 10
";
        let s = RNodeRfSettings::parse_rnode_section(raw).expect("section found");
        assert!(s.enabled);
        assert_eq!(s.port.as_deref(), Some("/dev/ttyACM0"));
        assert_eq!(s.frequency, 869_462_500);
        assert_eq!(s.bandwidth, 125_000);
        assert_eq!(s.spreading_factor, 8);
        assert_eq!(s.coding_rate, 5);
        assert_eq!(s.txpower, 14);
        assert_eq!(s.airtime_limit_short, Some(25.0));
        assert_eq!(s.airtime_limit_long, Some(10.0));
        s.validate().unwrap();
    }

    #[test]
    fn adoption_ignores_non_rnode_sections_and_absent_config() {
        let tcp_only = "\
[interfaces]
  [[Reticulum TCP Server]]
    type = TCPServerInterface
    listen_ip = 127.0.0.1
    listen_port = 4242
";
        assert!(RNodeRfSettings::parse_rnode_section(tcp_only).is_none());
        assert!(RNodeRfSettings::parse_rnode_section("").is_none());
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let base = RNodeRfSettings::default();
        for bad in [
            RNodeRfSettings {
                frequency: 100,
                ..base.clone()
            },
            RNodeRfSettings {
                bandwidth: 123_456,
                ..base.clone()
            },
            RNodeRfSettings {
                spreading_factor: 4,
                ..base.clone()
            },
            RNodeRfSettings {
                coding_rate: 9,
                ..base.clone()
            },
            RNodeRfSettings {
                txpower: 23,
                ..base.clone()
            },
            RNodeRfSettings {
                airtime_limit_short: Some(180.0),
                ..base.clone()
            },
            RNodeRfSettings {
                port: Some("ttyACM0".into()),
                ..base.clone()
            },
            RNodeRfSettings {
                port: Some("/dev/tty; rm -rf /".into()),
                ..base.clone()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?} should fail validation");
        }
    }
}
