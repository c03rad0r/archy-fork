//! Per-app version preferences — the persistence layer for multi-version support.
//!
//! Multi-version support (`docs/bitcoin-multi-version-design.md`) lets a node
//! runner pin Bitcoin Core / Knots to a specific version and opt into
//! auto-update-to-latest. Both choices live in the existing per-app config file
//! at `/var/lib/archipelago/app-configs/<id>.json` as two keys:
//!
//! ```jsonc
//! { "pinnedVersion": "29.3.knots20260508", "autoUpdate": false }
//! ```
//!
//! This is the single source of truth the orchestrator's install path reads to
//! resolve the image, and that the auto-update tick + "available update" badge
//! consult. Reads/writes are merge-preserving so they never clobber any
//! `containerConfig` (ports/volumes/env) a generic app may also store here.
//!
//! Platform-managed apps (bitcoin-core/knots/…) never use the
//! `containerConfig`-style keys (see `config.rs::dynamic_app_config`, which
//! returns early for them), so adding these keys to their file is collision-free.

use serde_json::{Map, Value};
use std::path::PathBuf;

/// Resolved version preferences for one app. Defaults: no pin, auto-update off
/// (consensus-critical apps opt in explicitly — design open-question #4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppVersionConfig {
    /// The version string the runner pinned, if any. Suppresses the update badge
    /// and overrides the catalog default at install/recreate time.
    pub pinned_version: Option<String>,
    /// When true, the hourly catalog tick updates this app to the catalog
    /// default automatically. Ignored while a version is pinned.
    pub auto_update: bool,
}

fn config_dir() -> PathBuf {
    let base = std::env::var("ARCHIPELAGO_DATA_DIR")
        .unwrap_or_else(|_| "/var/lib/archipelago".to_string());
    PathBuf::from(base).join("app-configs")
}

fn config_path(app_id: &str) -> PathBuf {
    config_dir().join(format!("{app_id}.json"))
}

/// App ids that have opted into auto-update-to-latest AND are not pinned (a pin
/// is an explicit "stay here"). Drives the hourly per-app auto-update tick. The
/// app id is the config file stem. Returns empty when the dir is absent.
pub fn auto_update_apps() -> Vec<String> {
    let mut out = Vec::new();
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
        let cfg = read(app_id);
        if cfg.auto_update && cfg.pinned_version.is_none() {
            out.push(app_id.to_string());
        }
    }
    out
}

fn read_raw(app_id: &str) -> Map<String, Value> {
    let path = config_path(app_id);
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str::<Value>(&s)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        Err(_) => Map::new(),
    }
}

/// Read the version preferences for `app_id`. Returns defaults when the file is
/// absent or the keys are unset.
pub fn read(app_id: &str) -> AppVersionConfig {
    let obj = read_raw(app_id);
    AppVersionConfig {
        pinned_version: obj
            .get("pinnedVersion")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from),
        auto_update: obj
            .get("autoUpdate")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// The pinned version for `app_id`, if set. Convenience for the hot path.
pub fn pinned_version(app_id: &str) -> Option<String> {
    read(app_id).pinned_version
}

/// Parse the leading numeric `major.minor.patch` of a version string into a
/// comparable tuple. Stops at the first non-numeric component, so Bitcoin Core
/// (`31.0`, `28.4`) and the Knots date-suffixed form (`29.3.knots20260508` →
/// `(29, 3, 0)`) both compare on their consensus-relevant major/minor. The
/// Knots build-date suffix is intentionally ignored — a same-major.minor Knots
/// rebuild is not a chainstate downgrade.
fn version_key(version: &str) -> (u64, u64, u64) {
    let mut it = version.split('.').map(|c| {
        // Take the leading digit run of each dotted component (`knots20260508`
        // yields no leading digits → 0; `3` → 3).
        c.chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .parse::<u64>()
            .unwrap_or(0)
    });
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// True when installing `candidate` over `current` is a DOWNGRADE — an older
/// Bitcoin release over a chainstate written by a newer one. This is the
/// highest-risk operation (Core refuses to start on a newer chainstate without
/// an expensive reindex; pruned nodes can lose data), so the UI must warn and
/// the switch must be explicitly confirmed (design §4). Equal or newer → false.
pub fn is_downgrade(current: &str, candidate: &str) -> bool {
    version_key(candidate) < version_key(current)
}

/// Merge `cfg` into the on-disk config, preserving every other key. A
/// `pinned_version` of `None` removes the `pinnedVersion` key (un-pins / "track
/// latest"). Creates the directory and file on first write.
pub fn write(app_id: &str, cfg: &AppVersionConfig) -> std::io::Result<()> {
    let path = config_path(app_id);
    let mut obj = read_raw(app_id);
    match &cfg.pinned_version {
        Some(v) => {
            obj.insert("pinnedVersion".to_string(), Value::String(v.clone()));
        }
        None => {
            obj.remove("pinnedVersion");
        }
    }
    obj.insert("autoUpdate".to_string(), Value::Bool(cfg.auto_update));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(&Value::Object(obj))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Atomic-ish write: temp + rename so a crash mid-write can't truncate config.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serialized.as_bytes())?;
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // `ARCHIPELAGO_DATA_DIR` is process-global, so the write/read tests must not
    // run concurrently — serialize them and give each a unique dir. Without this
    // lock, parallel `cargo test` races on the env var (poisoning is fine: a
    // panicking test still releases a usable guard).
    static ENV_LOCK: std::sync::Mutex<u64> = std::sync::Mutex::new(0);

    fn with_tmp_data_dir<F: FnOnce()>(f: F) {
        let mut counter = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        let dir =
            std::env::temp_dir().join(format!("archy-vc-test-{}-{}", std::process::id(), *counter));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("ARCHIPELAGO_DATA_DIR", &dir);
        f();
        std::env::remove_var("ARCHIPELAGO_DATA_DIR");
        let _ = std::fs::remove_dir_all(&dir);
        // `counter` guard drops here, releasing the lock for the next test.
    }

    #[test]
    fn defaults_when_absent() {
        with_tmp_data_dir(|| {
            let cfg = read("bitcoin-core");
            assert_eq!(cfg.pinned_version, None);
            assert!(!cfg.auto_update);
        });
    }

    #[test]
    fn write_then_read_roundtrips() {
        with_tmp_data_dir(|| {
            write(
                "bitcoin-knots",
                &AppVersionConfig {
                    pinned_version: Some("29.3.knots20260508".into()),
                    auto_update: false,
                },
            )
            .unwrap();
            let cfg = read("bitcoin-knots");
            assert_eq!(cfg.pinned_version.as_deref(), Some("29.3.knots20260508"));
            assert!(!cfg.auto_update);
        });
    }

    #[test]
    fn write_preserves_existing_keys() {
        with_tmp_data_dir(|| {
            // Simulate a generic app's containerConfig already on disk.
            let path = config_path("someapp");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, r#"{"ports":["80:80"],"autoUpdate":false}"#).unwrap();
            write(
                "someapp",
                &AppVersionConfig {
                    pinned_version: Some("1.2.3".into()),
                    auto_update: true,
                },
            )
            .unwrap();
            let raw = read_raw("someapp");
            assert!(raw.contains_key("ports"), "ports key must survive");
            assert_eq!(raw.get("pinnedVersion").unwrap(), "1.2.3");
            assert_eq!(raw.get("autoUpdate").unwrap(), &Value::Bool(true));
        });
    }

    #[test]
    fn downgrade_detection() {
        // Older over newer = downgrade.
        assert!(is_downgrade("31.0", "30.0"));
        assert!(is_downgrade("28.4", "27.2"));
        // Same or newer = not a downgrade.
        assert!(!is_downgrade("30.0", "31.0"));
        assert!(!is_downgrade("28.4", "28.4"));
        // Knots date-suffixed strings compare on major.minor only.
        assert!(is_downgrade("29.3.knots20260508", "28.1.knots20251010"));
        assert!(!is_downgrade("29.3.knots20260101", "29.3.knots20260508"));
    }

    #[test]
    fn unpin_removes_key() {
        with_tmp_data_dir(|| {
            write(
                "bitcoin-core",
                &AppVersionConfig {
                    pinned_version: Some("31.0".into()),
                    auto_update: true,
                },
            )
            .unwrap();
            write(
                "bitcoin-core",
                &AppVersionConfig {
                    pinned_version: None,
                    auto_update: true,
                },
            )
            .unwrap();
            let raw = read_raw("bitcoin-core");
            assert!(!raw.contains_key("pinnedVersion"));
            assert_eq!(read("bitcoin-core").pinned_version, None);
            assert!(read("bitcoin-core").auto_update);
        });
    }
}
