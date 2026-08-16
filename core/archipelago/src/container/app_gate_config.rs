//! Per-app operator override for the app gate's login requirement.
//!
//! The manifest declares each port's *default* policy (`auth: gated` = the
//! gate challenges, the new `auth: open` = the gate fronts the port but does
//! not challenge). This store holds the operator's runtime override — set
//! from Settings → app details — so a node owner can un-gate an app that
//! carries its own login (Gitea, BTCPay) or force the gate back onto an
//! `open` app, without editing manifests or waiting for a catalog re-sign.
//!
//! Lives in the same merge-preserving per-app JSON files as the version
//! preferences (`/var/lib/archipelago/app-configs/<app_id>.json`, key
//! `"gateEnabled"`). Absent key = follow the manifest default.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Map, Value};

fn config_dir() -> PathBuf {
    let base = std::env::var("ARCHIPELAGO_DATA_DIR")
        .unwrap_or_else(|_| "/var/lib/archipelago".to_string());
    PathBuf::from(base).join("app-configs")
}

fn config_path(app_id: &str) -> PathBuf {
    config_dir().join(format!("{app_id}.json"))
}

fn read_raw(app_id: &str) -> Map<String, Value> {
    match std::fs::read_to_string(config_path(app_id)) {
        Ok(s) => serde_json::from_str::<Value>(&s)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        Err(_) => Map::new(),
    }
}

/// The operator's gate override for one app. `None` = no override recorded —
/// the manifest default applies.
pub fn gate_override(app_id: &str) -> Option<bool> {
    read_raw(app_id).get("gateEnabled").and_then(Value::as_bool)
}

/// Every recorded override, keyed by app id (the config file stem). Used by
/// the gate's port-map build so one directory scan covers all apps.
pub fn all_gate_overrides() -> HashMap<String, bool> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(config_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(app_id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(v) = gate_override(app_id) {
            out.insert(app_id.to_string(), v);
        }
    }
    out
}

/// Set (`Some`) or clear (`None`) the override, preserving every other key in
/// the app's config file. Temp+rename so a crash mid-write can't truncate.
pub fn write_gate_override(app_id: &str, enabled: Option<bool>) -> std::io::Result<()> {
    let path = config_path(app_id);
    let mut obj = read_raw(app_id);
    match enabled {
        Some(v) => {
            obj.insert("gateEnabled".to_string(), Value::Bool(v));
        }
        None => {
            obj.remove("gateEnabled");
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(&Value::Object(obj))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serialized.as_bytes())?;
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_tmp_data_dir<T>(f: impl FnOnce() -> T) -> T {
        let dir = tempfile::tempdir().expect("tempdir");
        // Serialize env mutation across tests in this module.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("ARCHIPELAGO_DATA_DIR", dir.path());
        let out = f();
        std::env::remove_var("ARCHIPELAGO_DATA_DIR");
        out
    }

    #[test]
    fn absent_file_means_no_override() {
        with_tmp_data_dir(|| {
            assert_eq!(gate_override("gitea"), None);
            assert!(all_gate_overrides().is_empty());
        });
    }

    #[test]
    fn write_read_clear_roundtrip_preserves_other_keys() {
        with_tmp_data_dir(|| {
            // Seed an existing config with an unrelated key.
            std::fs::create_dir_all(config_dir()).unwrap();
            std::fs::write(config_path("gitea"), r#"{"autoUpdate": true}"#).unwrap();

            write_gate_override("gitea", Some(false)).unwrap();
            assert_eq!(gate_override("gitea"), Some(false));
            assert_eq!(all_gate_overrides().get("gitea"), Some(&false));

            // The unrelated key survives.
            let raw = std::fs::read_to_string(config_path("gitea")).unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v.get("autoUpdate"), Some(&Value::Bool(true)));

            write_gate_override("gitea", None).unwrap();
            assert_eq!(gate_override("gitea"), None);
            let raw = std::fs::read_to_string(config_path("gitea")).unwrap();
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v.get("autoUpdate"), Some(&Value::Bool(true)));
        });
    }
}
