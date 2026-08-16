//! Which app is behind a given host port, and may it be reached without
//! authenticating?
//!
//! The gate has to answer both questions for every inbound connection: the
//! first to decide whether to challenge at all, the second so the login page
//! can name and picture what the visitor is trying to open ("you are logging
//! in to reach Immich"), which is what makes the challenge legible instead of
//! alarming.
//!
//! Both answers come from the installed manifests rather than a generated
//! table, so a catalog refresh that adds or repoints an app is reflected
//! without a daemon restart — the same reason `app_port_v6_relay_loop`
//! rescans instead of snapshotting once.

use archipelago_container::manifest::{AppManifest, PortAuth};
use std::collections::HashMap;
use std::path::PathBuf;

/// An app port the gate is responsible for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatedPort {
    pub port: u16,
    pub app_id: String,
    /// Display name for the login page. Falls back to the id when a manifest
    /// omits `name`.
    pub app_name: String,
    /// Manifest-declared icon path (`metadata.icon`), when present.
    pub icon: Option<String>,
    /// True only when the manifest says `auth: gated` in so many words.
    ///
    /// The gated set deliberately also carries undeclared Session-default
    /// ports (so the gate challenges them wherever it can already stand, and
    /// the audit reports them). But everything that CHANGES where traffic
    /// goes — the torrc repoint to 127.0.0.2, the FIPS relay stand-down, the
    /// Tor-upstream bind — must key on this flag: acting on an undeclared
    /// port is the v1.7.121 incident class, whatever the action.
    pub declared: bool,
    /// Manifest opt-in (`session_passthrough: true` on the port): forward the
    /// node session cookie to the app on authorised requests. First-party
    /// companion UIs proxy that cookie to the daemon's authenticated
    /// endpoints; for every other app the gate strips its own credential.
    pub session_passthrough: bool,
    /// Does the gate challenge for the dashboard login on this port?
    ///
    /// Default comes from the manifest (`auth: gated`/undeclared → true,
    /// `auth: open` → false); the operator's runtime override
    /// (`app_gate_config`, Settings → app → App gate) wins over both.
    /// False does NOT release the port — the gate keeps binding and
    /// proxying (frame-header fixes, app-down page, Tor upstream); it just
    /// forwards every request to the app's own authentication.
    pub auth_enabled: bool,
}

/// A port deliberately left unauthenticated, and the manifest's stated reason.
///
/// Carried around rather than discarded because "which ports are open and
/// why" is the question an operator actually asks, and the answer should be
/// one RPC call rather than an audit of 56 YAML files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExemptPort {
    pub port: u16,
    pub app_id: String,
    pub rationale: String,
    /// UDP ports are listed for completeness. The gate is TCP-only, so it
    /// could not touch them even if they were marked `session`.
    pub protocol: String,
}

/// Everything the gate knows about the node's published surface.
#[derive(Debug, Clone, Default)]
pub struct PortMap {
    gated: HashMap<u16, GatedPort>,
    exempt: Vec<ExemptPort>,
    local: std::collections::HashSet<u16>,
}

impl PortMap {
    /// The app behind `port`, if the gate is responsible for it.
    pub fn gated(&self, port: u16) -> Option<&GatedPort> {
        self.gated.get(&port)
    }

    pub fn gated_ports(&self) -> impl Iterator<Item = &GatedPort> {
        self.gated.values()
    }

    pub fn exempt_ports(&self) -> &[ExemptPort] {
        &self.exempt
    }

    /// Declared `auth: local` — host-local by intent, so NOTHING may make it
    /// externally reachable.
    ///
    /// The gate honours this by keeping its hands off, but it is not the only
    /// thing that can publish a port: the FIPS mesh relay bridges the fips0
    /// ULA to `127.0.0.1` for a static port list, and it forwarded nbxplorer
    /// 32838 — declared `local` and pinned to loopback — to the mesh
    /// unauthenticated (test node 2026-08-04). Anything that republishes
    /// a loopback port must consult this set first.
    pub fn is_declared_local(&self, port: u16) -> bool {
        self.local.contains(&port)
    }

    pub fn is_empty(&self) -> bool {
        self.gated.is_empty() && self.exempt.is_empty() && self.local.is_empty()
    }
}

/// Directories searched for installed manifests, most specific first.
///
/// Mirrors `api::rpc::package::runtime::manifest_apps_dirs` deliberately: the
/// gate must classify exactly the manifests the orchestrator installs from,
/// or a port could be gated here and published from a different declaration
/// there.
fn apps_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        dirs.push(PathBuf::from(manifest_dir).join("../../apps"));
    }
    dirs.extend([
        PathBuf::from("apps"),
        PathBuf::from("/opt/archipelago/apps"),
        PathBuf::from("/opt/archipelago/web-ui/archipelago-runtime/apps"),
    ]);
    dirs
}

/// Read `metadata.icon` out of the manifest's untyped extension bag.
fn manifest_icon(manifest: &AppManifest) -> Option<String> {
    manifest
        .app
        .extensions
        .get("metadata")?
        .get("icon")?
        .as_str()
        .map(str::to_string)
}

/// Classify every published port across all installed manifests.
///
/// The signed catalog's embedded manifests are consulted FIRST, because they
/// are what the orchestrator actually publishes containers from
/// (origin-wins; see `app_catalog::catalog_manifest_overlay`). Classifying
/// from disk alone made the gate act on policy the node was no longer
/// running: the catalog declared nbxplorer `auth: local` and pinned it to
/// loopback, the stale disk manifest declared nothing, and the gate
/// externally bound a deliberately host-local port (a test node
/// 2026-08-04).
///
/// After the catalog, the first directory that yields a manifest for an app
/// id wins, so a node's `/opt/archipelago/apps` copy shadows a repo checkout
/// rather than merging with it — otherwise a stale checked-out manifest could
/// re-open a port the installed one gates.
pub fn build_port_map() -> PortMap {
    let mut map = PortMap::default();
    let mut seen_apps: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (app_id, value) in crate::container::app_catalog::catalog_manifest_values() {
        // Ports-only overlay: unlike the install path, classification also
        // accepts BUILD-SOURCE manifests. The on-node-built companion UIs
        // are exactly the apps whose gate policy (session_passthrough,
        // auth: gated) must arrive reliably, and their disk manifests
        // proved stale or absent fleet-wide in the v1.7.125 rollout. The
        // gate's binds fail safely on conflict with a differently-published
        // container, so a fresher catalog can only tighten, never expose.
        let Some(manifest) =
            crate::container::app_catalog::catalog_manifest_ports_overlay(&app_id, value)
        else {
            // Unparseable/invalid → the orchestrator falls back to disk for
            // this app, so classification must too.
            continue;
        };
        if seen_apps.insert(app_id) {
            classify_manifest(&manifest, &mut map);
        }
    }

    for dir in apps_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path().join("manifest.yml");
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(manifest) = AppManifest::parse(&contents) else {
                // A manifest that does not parse is not installable either,
                // so skipping it cannot open a port that the orchestrator
                // would have published.
                continue;
            };
            if seen_apps.insert(manifest.app.id.clone()) {
                classify_manifest(&manifest, &mut map);
            }
        }
    }

    // The operator's runtime override wins over the manifest default, in
    // both directions: un-gate an app that fronts its own login, or force
    // the challenge back onto an `auth: open` port. Overrides only toggle
    // the challenge on gate-fronted ports — they never bind or release
    // anything, so a stale override cannot expose or strand a port.
    let overrides = crate::container::app_gate_config::all_gate_overrides();
    for gp in map.gated.values_mut() {
        if let Some(enabled) = overrides.get(&gp.app_id) {
            gp.auth_enabled = *enabled;
        }
    }

    map.exempt.sort_by_key(|e| e.port);
    map
}

/// Classify one manifest's ports into the map. Split from [`build_port_map`]
/// so the catalog-overlay pass and the disk pass cannot diverge.
fn classify_manifest(manifest: &AppManifest, map: &mut PortMap) {
    let app_id = manifest.app.id.clone();
    let icon = manifest_icon(manifest);
    let app_name = if manifest.app.name.trim().is_empty() {
        app_id.clone()
    } else {
        manifest.app.name.clone()
    };

    for port in &manifest.app.ports {
        let protocol = if port.protocol.is_empty() {
            "tcp"
        } else {
            port.protocol.as_str()
        };
        match port.auth_policy() {
            PortAuth::None => map.exempt.push(ExemptPort {
                port: port.host,
                app_id: app_id.clone(),
                rationale: port
                    .auth_rationale
                    .clone()
                    .unwrap_or_else(|| "(no rationale recorded)".to_string()),
                protocol: protocol.to_string(),
            }),
            // Declared host-local. Not gated and not reported as
            // exposed, because it is neither — see PortAuth::Local
            // for why this cannot be inferred from `bind`. Recorded so
            // the mesh relay (and any future republisher) can refuse to
            // expose it.
            PortAuth::Local => {
                map.local.insert(port.host);
            }
            // Explicit opt-in: the app is on loopback and the daemon
            // owns the external addresses. This is the ONLY way a
            // port gets bound by the gate, regardless of `bind`.
            // `open` is the same takeover with the login challenge
            // defaulted off — the app fronts its own authentication
            // (rationale-required, see PortAuth::Open).
            PortAuth::Gated | PortAuth::Open => {
                map.gated.insert(
                    port.host,
                    GatedPort {
                        port: port.host,
                        app_id: app_id.clone(),
                        app_name: app_name.clone(),
                        icon: icon.clone(),
                        declared: true,
                        session_passthrough: port.session_passthrough,
                        auth_enabled: port.auth_policy() == PortAuth::Gated,
                    },
                );
            }
            PortAuth::Session => {
                // UDP cannot carry an HTTP challenge. Such a port has
                // no business defaulting into the gated set where it
                // would look protected without being protectable —
                // surface it as an unrationalised exemption instead,
                // which is honest and shows up in the audit list.
                if protocol != "tcp" {
                    map.exempt.push(ExemptPort {
                        port: port.host,
                        app_id: app_id.clone(),
                        rationale: format!(
                            "{protocol} cannot carry an HTTP challenge; declare auth: none \
                             with a rationale to record why this is safe"
                        ),
                        protocol: protocol.to_string(),
                    });
                    continue;
                }
                // A loopback publish is skipped, and this is the
                // safety property of the whole module: the gate must
                // never be the reason a port becomes reachable
                // somewhere it was not. `session` is the DEFAULT, so
                // it is what every un-migrated manifest carries —
                // and a node's installed manifests always lag the
                // repo. Binding those externally published Bitcoin
                // RPC across the LAN within seconds of deploy
                // (test node 2026-08-03). Taking over a port is
                // opt-in only: `auth: gated`, shipped in the same
                // manifest edit as the loopback pin.
                if port
                    .bind
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
                {
                    continue;
                }
                map.gated.insert(
                    port.host,
                    GatedPort {
                        port: port.host,
                        app_id: app_id.clone(),
                        app_name: app_name.clone(),
                        icon: icon.clone(),
                        declared: false,
                        // An undeclared port never gets the node session —
                        // passthrough is an explicit manifest opt-in only.
                        session_passthrough: false,
                        // Undeclared ports are challenged wherever the gate
                        // can stand: reporting-and-protecting is the safe
                        // default (operator override still applies below).
                        auth_enabled: true,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus this runs against is the real `apps/` tree, so these assert
    /// on properties rather than exact contents — the set of apps changes,
    /// the invariants must not.
    #[test]
    fn real_manifests_classify_into_both_sets() {
        let map = build_port_map();
        assert!(!map.is_empty(), "no manifests found — apps dir missing?");
        assert!(
            map.gated_ports().count() > 20,
            "expected most published ports to be gated, got {}",
            map.gated_ports().count()
        );
        assert!(!map.exempt_ports().is_empty());
    }

    #[test]
    fn every_exemption_carries_a_reason() {
        for exempt in build_port_map().exempt_ports() {
            assert!(
                !exempt.rationale.trim().is_empty(),
                "port {} ({}) is exempt with no rationale",
                exempt.port,
                exempt.app_id
            );
        }
    }

    fn manifest(yaml: &str) -> AppManifest {
        AppManifest::parse(yaml).expect("test manifest must parse")
    }

    const BASE: &str = r#"
app:
  id: testapp
  name: Test App
  version: "1.0"
  container:
    image: example.org/testapp:1.0
"#;

    /// `auth: gated` is the only classification allowed to redirect traffic —
    /// torrc repoints, relay stand-down, and the 127.0.0.2 bind all key on
    /// `declared`. An undeclared Session port is challenged and audited but
    /// must never be `declared`.
    #[test]
    fn declared_tracks_the_manifest_not_the_default() {
        let mut map = PortMap::default();
        classify_manifest(
            &manifest(&format!(
                "{BASE}  ports:\n    - host: 8090\n      container: 7777\n      protocol: tcp\n      bind: 127.0.0.1\n      auth: gated\n"
            )),
            &mut map,
        );
        assert!(map.gated(8090).expect("gated").declared);

        let mut map = PortMap::default();
        classify_manifest(
            &manifest(&format!(
                "{BASE}  ports:\n    - host: 9100\n      container: 9100\n      protocol: tcp\n"
            )),
            &mut map,
        );
        let undeclared = map.gated(9100).expect("session default is challenged");
        assert!(
            !undeclared.declared,
            "an absent auth field must never read as an instruction"
        );
    }

    /// `auth: local` keeps the gate's hands off entirely — the port is
    /// neither gated nor exempt-reported — but it IS recorded, so the mesh
    /// relay can refuse to republish a deliberately host-local port.
    #[test]
    fn local_ports_are_untouched_but_recorded() {
        let mut map = PortMap::default();
        classify_manifest(
            &manifest(&format!(
                "{BASE}  ports:\n    - host: 32838\n      container: 32838\n      protocol: tcp\n      bind: 127.0.0.1\n      auth: local\n"
            )),
            &mut map,
        );
        assert!(map.gated(32838).is_none());
        assert!(map.exempt_ports().is_empty());
        assert!(
            map.is_declared_local(32838),
            "the mesh relay needs this to refuse bridging a host-local port"
        );
        assert!(!map.is_declared_local(3000));
    }

    /// The real corpus: every port the FIPS relay can bridge must be safe to
    /// bridge. A port that is declared `local` (host-local by intent) or
    /// declared `gated` (the app gate owns its external addresses) must be
    /// withheld by the relay — this asserts the two sets the relay consults
    /// actually classify the live manifests, so a future manifest edit that
    /// re-opens one is caught here rather than on a node.
    #[test]
    fn relay_port_list_respects_local_and_gated_declarations() {
        let map = build_port_map();
        let relay_would_expose: Vec<u16> = crate::fips::app_ports::APP_LAUNCH_PORTS
            .iter()
            .copied()
            .filter(|p| map.is_declared_local(*p))
            .collect();
        assert!(
            !relay_would_expose.is_empty(),
            "expected the corpus to contain at least one local port in the relay list \
             (32838/8999) — if this fails the guard is untested, not unnecessary"
        );
    }

    /// Protocol ports that wallets dial directly must never end up gated —
    /// this is the constraint that decided the design (Zeus and electrum
    /// clients keep working untouched).
    #[test]
    fn wallet_protocol_ports_are_not_gated() {
        let map = build_port_map();
        for port in [10009, 18080, 9735, 50001] {
            assert!(
                map.gated(port).is_none(),
                "port {port} must stay ungated — remote wallets cannot hold a session"
            );
        }
    }

    /// Bitcoin's RPC is host-local by intent (`auth: local`), so the gate
    /// must neither gate it nor report it as exposed — fronting it would
    /// newly publish it on every host address, behind a login but reachable
    /// where it deliberately was not.
    #[test]
    fn host_local_ports_are_neither_gated_nor_reported() {
        let map = build_port_map();
        assert!(map.gated(8332).is_none(), "bitcoin RPC must not be gated");
        assert!(
            !map.exempt_ports().iter().any(|e| e.port == 8332),
            "a host-local port is not an unauthenticated exposure"
        );
    }

    /// THE safety property. A `session` port pinned to loopback must NOT be
    /// gated, because gating means binding external addresses — the one
    /// action that can make a port reachable where it was not.
    ///
    /// This is not hypothetical. `session` is the default, so it is what
    /// every un-migrated manifest carries, and a node's installed manifests
    /// always lag the repo. An earlier revision gated these regardless of
    /// `bind`, and within seconds of deploying to a test node the daemon
    /// had published Bitcoin's loopback-only RPC 8332 on the LAN, Tailscale
    /// and IPv6 addresses. Taking over a port must be opt-in.
    #[test]
    fn a_loopback_pinned_session_port_is_never_gated() {
        let map = build_port_map();
        // aiui and bitcoin RPC are both loopback-pinned in the shipped tree.
        for port in [5180, 8332] {
            assert!(
                map.gated(port).is_none(),
                "port {port} is loopback-pinned; gating it would newly expose it"
            );
        }
    }

    /// The migration end state: `auth: gated` opts a loopback-pinned port
    /// into daemon ownership. Without this the rollout could never complete.
    #[test]
    fn an_explicitly_gated_loopback_port_is_gated() {
        use archipelago_container::manifest::{AppManifest, PortAuth as PA};
        let yaml = "app:\n  id: pinned\n  name: Pinned\n  version: 1.0.0\n  container:\n    image: x:y\n  ports:\n    - host: 9911\n      container: 80\n      bind: 127.0.0.1\n      auth: gated\n";
        let m = AppManifest::parse(yaml).expect("parses");
        assert_eq!(m.app.ports[0].auth, Some(PA::Gated));
        assert_eq!(m.app.ports[0].bind, "127.0.0.1");
    }

    /// An app UI that was reachable with no credential in the 2026-08-03
    /// reproduction must now resolve to a gated port with a display name.
    #[test]
    fn reproduced_open_ports_are_now_gated() {
        let map = build_port_map();
        let strfry = map.gated(8090).expect("strfry :8090 must be gated");
        assert_eq!(strfry.app_id, "strfry");
        assert!(!strfry.app_name.is_empty());
    }
}
