//! Web-UI detection for discovered containers.
//!
//! The packages list used to mark every container that published a port (or
//! carried an onion address) as a UI app, which gave headless backends —
//! databases, media servers, self-deployed compose stacks — a Launch button.
//! UI-ness is decided here instead, for manifest apps and ad-hoc containers
//! alike:
//!
//! 1. A manifest that declares an `interfaces:` block is definitive: any
//!    entry of `type: ui` means a browsable UI, a block without one means a
//!    backend service. The signed catalog overlay is consulted before disk
//!    manifests (catalog supremacy).
//! 2. Manifests without an `interfaces:` block (the overwhelming majority)
//!    and manifest-less containers fall through to a short HTTP probe of the
//!    launch port: an HTML page, a redirect, or a browser-auth wall means a
//!    UI; JSON APIs, raw TCP protocols, and dead ports mean a service.
//!
//! Verdicts are cached — positives longer than negatives, since "no UI yet"
//! is often just an app that hasn't finished starting.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tracing::debug;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// A confirmed UI stays a UI — re-check rarely.
const POSITIVE_TTL: Duration = Duration::from_secs(15 * 60);
/// A "no UI" verdict may be a slow-starting app — re-check sooner.
const NEGATIVE_TTL: Duration = Duration::from_secs(2 * 60);

/// Decide whether `app_id` exposes a browsable web UI. `lan_address` is the
/// launch candidate already computed by the package scanner (host-published),
/// and `running` gates the probe: a stopped container can't answer, and a
/// dead-port verdict against it would poison the cache.
pub async fn has_web_ui(app_id: &str, lan_address: Option<&str>, running: bool) -> bool {
    if let Some(declared) = manifest_declares_ui(app_id) {
        return declared;
    }
    let Some(port) = lan_address.and_then(super::docker_packages::launch_url_port) else {
        return false;
    };
    if !running {
        // Serve a cached verdict if we have one, but never record one.
        return cached_verdict(app_id, port).unwrap_or(false);
    }
    if let Some(v) = cached_verdict(app_id, port) {
        return v;
    }
    let verdict = probe_port(port).await;
    debug!(app_id, port, verdict, "web-UI probe");
    cache_verdict(app_id, port, verdict);
    verdict
}

// ─── Manifest declarations ───────────────────────────────────────────────

/// `Some(true)`: a manifest covers this app and declares a `type: ui`
/// interface. `Some(false)`: a manifest declares interfaces, none of them UI.
/// `None`: no manifest, or one without an `interfaces:` block — undeclared,
/// let the probe decide.
fn manifest_declares_ui(app_id: &str) -> Option<bool> {
    // Catalog overlay first: disk manifests don't apply to catalog-covered
    // apps, so the catalog must win where it speaks.
    for (id, value) in super::app_catalog::catalog_manifest_values() {
        if id == app_id {
            if let Some(v) = declared_ui_in_value(&value) {
                return Some(v);
            }
            break;
        }
    }
    for path in disk_manifest_candidates(app_id) {
        if !path.exists() {
            continue;
        }
        match archipelago_container::AppManifest::from_file(&path) {
            Ok(m) => {
                if m.app.interfaces.is_empty() {
                    return None;
                }
                return Some(m.app.interfaces.values().any(|i| i.interface_type == "ui"));
            }
            // Malformed manifests are already reported by the orchestrator's
            // loader; here they simply don't count as a declaration.
            Err(_) => return None,
        }
    }
    None
}

/// Same declaration logic against a catalog manifest carried as raw JSON.
fn declared_ui_in_value(manifest: &serde_json::Value) -> Option<bool> {
    let interfaces = manifest.get("app")?.get("interfaces")?.as_object()?;
    if interfaces.is_empty() {
        return None;
    }
    Some(interfaces.values().any(|i| {
        // An omitted `type` defaults to "ui", mirroring the YAML schema.
        i.get("type")
            .and_then(|t| t.as_str())
            .map(|t| t == "ui")
            .unwrap_or(true)
    }))
}

fn disk_manifest_candidates(app_id: &str) -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(v) = std::env::var("ARCHIPELAGO_APPS_DIR") {
        let v = v.trim();
        if !v.is_empty() {
            roots.push(v.into());
        }
    }
    roots.push("/opt/archipelago/apps".into());
    roots
        .into_iter()
        .map(|root| root.join(app_id).join("manifest.yml"))
        .collect()
}

// ─── HTTP probe ──────────────────────────────────────────────────────────

async fn probe_port(port: u16) -> bool {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(PROBE_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("static probe client")
    });
    match client.get(format!("http://127.0.0.1:{port}/")).send().await {
        Ok(resp) => {
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let auth_challenge = resp
                .headers()
                .contains_key(reqwest::header::WWW_AUTHENTICATE);
            ui_verdict(resp.status().as_u16(), content_type, auth_challenge)
        }
        // Connection refused, timeout, or a non-HTTP protocol on the port.
        Err(_) => false,
    }
}

/// The pure classification rule, split out for tests: a browsable UI is an
/// HTML response (any status — error pages included), a redirect (login
/// flows), or a browser auth prompt. JSON/plaintext APIs are services.
fn ui_verdict(status: u16, content_type: &str, auth_challenge: bool) -> bool {
    if content_type.contains("text/html") {
        return true;
    }
    if (300..400).contains(&status) {
        return true;
    }
    if (status == 401 || status == 403) && auth_challenge {
        return true;
    }
    false
}

// ─── Verdict cache ───────────────────────────────────────────────────────

fn cache() -> &'static Mutex<HashMap<String, (bool, Instant)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (bool, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn cached_verdict(app_id: &str, port: u16) -> Option<bool> {
    let cache = cache().lock().ok()?;
    let (verdict, at) = cache.get(&format!("{app_id}:{port}"))?;
    let ttl = if *verdict { POSITIVE_TTL } else { NEGATIVE_TTL };
    (at.elapsed() < ttl).then_some(*verdict)
}

fn cache_verdict(app_id: &str, port: u16, verdict: bool) {
    if let Ok(mut cache) = cache().lock() {
        cache.insert(format!("{app_id}:{port}"), (verdict, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn html_is_ui_regardless_of_status() {
        assert!(ui_verdict(200, "text/html; charset=utf-8", false));
        assert!(ui_verdict(404, "text/html", false));
        assert!(ui_verdict(401, "text/html", false));
    }

    #[test]
    fn redirects_are_ui() {
        assert!(ui_verdict(302, "", false));
        assert!(ui_verdict(307, "text/plain", false));
    }

    #[test]
    fn auth_walls_are_ui_only_with_challenge() {
        assert!(ui_verdict(401, "text/plain", true));
        assert!(!ui_verdict(401, "application/json", false));
    }

    #[test]
    fn apis_and_plain_ports_are_services() {
        assert!(!ui_verdict(200, "application/json", false));
        assert!(!ui_verdict(200, "text/plain", false));
        assert!(!ui_verdict(500, "", false));
    }

    #[test]
    fn catalog_value_declaration() {
        let ui = json!({"app": {"interfaces": {"main": {"type": "ui", "port": 80}}}});
        assert_eq!(declared_ui_in_value(&ui), Some(true));

        // Omitted type defaults to "ui", mirroring the YAML schema default.
        let default_ty = json!({"app": {"interfaces": {"main": {"port": 80}}}});
        assert_eq!(declared_ui_in_value(&default_ty), Some(true));

        let api_only = json!({"app": {"interfaces": {"rpc": {"type": "api", "port": 80}}}});
        assert_eq!(declared_ui_in_value(&api_only), Some(false));

        // No interfaces block ⇒ undeclared, not "no UI".
        let none = json!({"app": {"id": "x"}});
        assert_eq!(declared_ui_in_value(&none), None);
        let empty = json!({"app": {"interfaces": {}}});
        assert_eq!(declared_ui_in_value(&empty), None);
    }

    #[test]
    fn verdict_cache_roundtrip() {
        cache_verdict("test-app", 1234, true);
        assert_eq!(cached_verdict("test-app", 1234), Some(true));
        assert_eq!(cached_verdict("test-app", 9999), None);
    }
}
