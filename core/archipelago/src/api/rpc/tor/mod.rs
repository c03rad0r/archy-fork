mod handlers;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

use crate::{federation, identity};

pub(super) const TOR_DATA_DIR: &str = "/var/lib/archipelago/tor";
pub(super) const SERVICES_CONFIG: &str = "services.json";
/// How long old service directories are kept during transition (seconds).
pub(super) const ROTATION_TRANSITION_SECS: u64 = 86400; // 24 hours

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TorService {
    pub name: String,
    pub local_port: u16,
    pub onion_address: Option<String>,
    pub enabled: bool,
    pub unauthenticated: bool,
    pub protocol: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(in crate::api::rpc) struct ServicesConfig {
    pub services: Vec<TorServiceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::api::rpc) struct TorServiceEntry {
    pub name: String,
    pub local_port: u16,
    #[serde(default)]
    pub remote_port: Option<u16>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub unauthenticated: bool,
}

fn default_true() -> bool {
    true
}

// ─── Validation ───────────────────────────────────────────────────

pub(super) fn validate_service_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(anyhow::anyhow!(
            "Invalid service name (alphanumeric, hyphens, underscores only)"
        ));
    }
    Ok(())
}

// ─── Tor Daemon Control ──────────────────────────────────────────

const TOR_ACTION_FILE: &str = "/var/lib/archipelago/tor-config/tor-action";
const TOR_RESULT_FILE: &str = "/var/lib/archipelago/tor-config/tor-result";

/// Write an action file and wait for the tor-helper service to process it.
pub(super) async fn dispatch_tor_action(action: serde_json::Value) -> Result<()> {
    let _ = tokio::fs::remove_file(TOR_RESULT_FILE).await;

    let content = serde_json::to_string(&action).context("Failed to serialize tor action")?;
    let config_dir = Path::new(TOR_ACTION_FILE)
        .parent()
        .unwrap_or_else(|| Path::new("/var/lib/archipelago/tor-config"));
    tokio::fs::create_dir_all(config_dir).await.ok();
    tokio::fs::write(TOR_ACTION_FILE, &content)
        .await
        .context("Failed to write tor-action file")?;

    for _ in 0..90 {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        if let Ok(result_str) = tokio::fs::read_to_string(TOR_RESULT_FILE).await {
            let _ = tokio::fs::remove_file(TOR_RESULT_FILE).await;
            if let Ok(result) = serde_json::from_str::<serde_json::Value>(&result_str) {
                if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Ok(());
                }
                let err = result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error");
                return Err(anyhow::anyhow!("Tor helper: {}", err));
            }
            return Ok(());
        }
    }
    Err(anyhow::anyhow!(
        "Tor helper timed out — is archipelago-tor-helper.path enabled?"
    ))
}

pub(super) async fn delete_hidden_service_dir(name: &str) {
    if let Err(e) = dispatch_tor_action(serde_json::json!({
        "action": "delete-service",
        "name": name,
    }))
    .await
    {
        warn!("Failed to delete hidden service dir for {}: {}", name, e);
    }
}

pub(super) async fn rename_hidden_service_dir(name: &str, timestamp: u64) {
    if let Err(e) = dispatch_tor_action(serde_json::json!({
        "action": "rename-service",
        "name": name,
        "timestamp": timestamp,
    }))
    .await
    {
        warn!("Failed to rename hidden service dir for {}: {}", name, e);
    }
}

pub(in crate::api::rpc) async fn restart_tor() -> Result<()> {
    dispatch_tor_action(serde_json::json!({
        "action": "write-torrc-and-restart",
    }))
    .await
}

pub(crate) async fn check_tor_running() -> bool {
    tokio::net::TcpStream::connect("127.0.0.1:9050")
        .await
        .is_ok()
}

// ─── torrc Generation ────────────────────────────────────────────

pub(super) fn detect_hidden_service_base() -> String {
    if Path::new("/var/lib/tor/hidden_service_archipelago").exists() {
        return "/var/lib/tor".to_string();
    }
    let custom = tor_data_dir();
    if Path::new(&custom)
        .join("hidden_service_archipelago")
        .exists()
    {
        return custom;
    }
    "/var/lib/tor".to_string()
}

/// The `archy-net` bridge's gateway address and subnet, if the network exists.
///
/// Derived at runtime, never hardcoded: `archy-net` is created without an
/// explicit subnet (see docker-compose.yml), so podman allocates one — it is
/// 10.89.0.0/24 on a test node but there is no guarantee of that on another
/// node, and a hardcoded guess would fail silently by binding SOCKS to an
/// address no container can reach.
///
/// Returns `None` when the network is absent or unparseable, which callers must
/// treat as "do not widen SOCKS" — failing closed keeps Tor loopback-only
/// rather than guessing at an exposure boundary.
async fn archy_net_gateway_and_subnet() -> Option<(String, String)> {
    let out = tokio::process::Command::new("podman")
        .args([
            "network",
            "inspect",
            "archy-net",
            "--format",
            "{{range .Subnets}}{{.Gateway}} {{.Subnet}}{{end}}",
        ])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split_whitespace();
    let gateway = parts.next()?.to_string();
    let subnet = parts.next()?.to_string();
    if gateway.is_empty() || subnet.is_empty() {
        return None;
    }
    // Podman REPORTING a gateway is not proof the host can bind it. Under
    // rootless podman the bridge and its gateway live inside a network
    // namespace, so the address never appears on a host interface — and Tor
    // binds it at startup, not at config-check time (`--verify-config` passes
    // happily). The result on austin-sapien, 2026-08-09:
    //
    //   [warn] Could not bind to 10.89.0.1:9050: Cannot assign requested address
    //   [warn] Failed to parse/validate config: Failed to bind one of the listener ports.
    //   [err]  Reading config failed--see warnings above.
    //
    // Tor then refuses to start AT ALL — loopback SOCKS and every hidden
    // service go with it. Widening SOCKS must never be able to take the whole
    // daemon down, so probe the address exactly the way Tor will and fall back
    // to the loopback-only branch when it is not bindable.
    if !host_can_bind(&gateway) {
        tracing::warn!(
            %gateway,
            "archy-net gateway is not bindable on this host (rootless podman \
             namespaces the bridge) — keeping Tor SOCKS loopback-only"
        );
        return None;
    }
    Some((gateway, subnet))
}

/// Boot-time Tor self-heal: rebuild torrc from current config and apply it if
/// the live file has drifted or Tor is not actually answering.
///
/// Without this, shipping a torrc-generation fix does NOT repair the nodes it
/// was written for. `regenerate_torrc` only runs from the Tor RPC handlers and
/// package install, so a node carrying a bad torrc keeps it until somebody
/// happens to install an app or toggle a Tor setting — its Tor stays down
/// indefinitely and an OTA changes nothing. That is exactly what happened with
/// the unbindable `SocksPort <archy-net gateway>` line: two of three reachable
/// nodes had Tor dead, one of them unnoticed.
///
/// Deliberately conservative — it only applies (which restarts Tor) when the
/// staged torrc differs from the live one or Tor is not answering on 9050.
/// A healthy node with a matching torrc is left completely alone.
pub(crate) async fn heal_on_boot(data_dir: &Path) -> Result<bool> {
    let config_dir = data_dir.join("tor-config");
    let config = load_services_config(&config_dir).await;

    // Stage what the torrc SHOULD be with the current (fixed) generator.
    regenerate_torrc(&config).await?;

    let staged = tokio::fs::read_to_string("/var/lib/archipelago/tor-config/torrc.staged")
        .await
        .unwrap_or_default();
    if staged.is_empty() {
        return Ok(false);
    }
    let live = tokio::fs::read_to_string("/etc/tor/torrc")
        .await
        .unwrap_or_default();

    let drifted = live.trim() != staged.trim();
    let answering = check_tor_running().await;
    if !drifted && answering {
        debug!("Tor healthy and torrc in sync — nothing to heal");
        return Ok(false);
    }

    info!(
        drifted,
        answering, "Healing Tor: rewriting torrc from current config and restarting"
    );
    restart_tor().await?;

    // Report the truth rather than assuming the restart worked.
    let ok = check_tor_running().await;
    if !ok {
        warn!("Tor still not answering on 127.0.0.1:9050 after heal");
    }
    Ok(ok)
}

/// Can this host actually bind `addr`? Binds an ephemeral port, the same
/// operation Tor performs, so the answer matches Tor's own behaviour rather
/// than inferring it from interface listings.
fn host_can_bind(addr: &str) -> bool {
    use std::net::{IpAddr, SocketAddr, TcpListener};
    match addr.parse::<IpAddr>() {
        Ok(ip) => TcpListener::bind(SocketAddr::new(ip, 0)).is_ok(),
        Err(_) => false,
    }
}

pub(in crate::api::rpc) async fn regenerate_torrc(config: &ServicesConfig) -> Result<()> {
    let base = detect_hidden_service_base();
    let mut lines = vec![
        "# Auto-generated by Archipelago — do not edit manually".to_string(),
        "SocksPort 9050".to_string(),
    ];

    // Containerised apps (Bitcoin Core/Knots) sit on the archy-net bridge, so
    // 127.0.0.1:9050 inside them is their OWN loopback — the host's Tor is
    // genuinely unreachable, which is why Core reported `onion reachable=False,
    // proxy=''` with every peer on clearnet. Bind a second SOCKS listener on the
    // bridge gateway so those containers have a Tor path at all.
    //
    // SocksPolicy is applied as an explicit accept-list terminated by a reject.
    // 127.0.0.1 MUST be accepted first: SocksPolicy applies to every SocksPort,
    // so an accept-list naming only the bridge subnet would lock the daemon out
    // of its own loopback SOCKS — the node's Tor usage would break in a way that
    // looks nothing like "we added a listener".
    match archy_net_gateway_and_subnet().await {
        Some((gateway, subnet)) => {
            lines.push(format!("SocksPort {gateway}:9050"));
            lines.push("SocksPolicy accept 127.0.0.1/32".to_string());
            lines.push(format!("SocksPolicy accept {subnet}"));
            lines.push("SocksPolicy reject *".to_string());
        }
        None => {
            lines.push(
                "# archy-net not found — SOCKS stays loopback-only (fail closed)".to_string(),
            );
        }
    }

    lines.push("# ControlPort disabled for security".to_string());
    lines.push(String::new());

    // Ports whose manifests declare `auth: gated` forward to the gate's own
    // loopback (127.0.0.2, where the app-gate listener binds — see
    // `appgate::listener::GATE_TOR_UPSTREAM`) instead of the app's 127.0.0.1.
    // Tor carries no session cookie, so an onion pointed at the app is an
    // unauthenticated bypass of the gate. Declared-gated ports only: an
    // undeclared port keeps today's target, because absence of the field is
    // not an instruction (the v1.7.121 incident rule).
    let gated_ports: std::collections::HashSet<u16> = crate::appgate::identity::build_port_map()
        .gated_ports()
        .filter(|g| g.declared)
        .map(|g| g.port)
        .collect();

    for svc in &config.services {
        if !svc.enabled {
            continue;
        }
        let dir = format!("{}/hidden_service_{}", base, svc.name);
        lines.push(format!("HiddenServiceDir {}", dir));

        if is_protocol_service(&svc.name) {
            let remote_port = svc.remote_port.unwrap_or(svc.local_port);
            lines.push(format!(
                "HiddenServicePort {} 127.0.0.1:{}",
                remote_port, svc.local_port
            ));
            if svc.name == "lnd" {
                lines.push("HiddenServicePort 9735 127.0.0.1:9735".to_string());
                lines.push("HiddenServicePort 10009 127.0.0.1:10009".to_string());
            }
        } else {
            lines.push(app_hidden_service_port_line(svc.local_port, &gated_ports));
        }

        lines.push(String::new());
    }

    let content = lines.join("\n");
    let staging = "/var/lib/archipelago/tor-config/torrc.staged";
    write_staged_torrc(&content, staging).await
}

/// The `HiddenServicePort` line for an HTTP app onion. Gated ports forward to
/// the gate's Tor upstream; everything else to the app itself.
fn app_hidden_service_port_line(
    local_port: u16,
    gated_ports: &std::collections::HashSet<u16>,
) -> String {
    let upstream = if gated_ports.contains(&local_port) {
        crate::appgate::listener::GATE_TOR_UPSTREAM.to_string()
    } else {
        "127.0.0.1".to_string()
    };
    format!("HiddenServicePort 80 {}:{}", upstream, local_port)
}

async fn write_staged_torrc(content: &str, staging: &str) -> Result<()> {
    let config_dir = Path::new(staging)
        .parent()
        .unwrap_or_else(|| Path::new("/var/lib/archipelago/tor-config"));
    tokio::fs::create_dir_all(config_dir).await.ok();
    tokio::fs::write(staging, &content)
        .await
        .context("Failed to write staged torrc")?;

    debug!("Staged torrc ({} bytes)", content.len());

    Ok(())
}

#[cfg(test)]
mod torrc_tests {
    use super::app_hidden_service_port_line;
    use std::collections::HashSet;

    #[test]
    fn gated_port_forwards_to_the_gate_not_the_app() {
        let gated: HashSet<u16> = [8082u16].into_iter().collect();
        assert_eq!(
            app_hidden_service_port_line(8082, &gated),
            "HiddenServicePort 80 127.0.0.2:8082"
        );
    }

    #[test]
    fn undeclared_port_keeps_the_app_loopback_target() {
        // Absence of `auth: gated` is not an instruction — the onion keeps
        // pointing at the app, exactly as before this change.
        let gated: HashSet<u16> = [8082u16].into_iter().collect();
        assert_eq!(
            app_hidden_service_port_line(9100, &gated),
            "HiddenServicePort 80 127.0.0.1:9100"
        );
    }
}

// ─── Hostname Sync ───────────────────────────────────────────────

pub(in crate::api::rpc) async fn sync_single_hostname(name: &str, address: &str) {
    let hostnames_dir = Path::new("/var/lib/archipelago/tor-hostnames");
    if let Err(e) = tokio::fs::create_dir_all(hostnames_dir).await {
        warn!("Failed to create tor-hostnames dir: {}", e);
        return;
    }
    if let Err(e) = tokio::fs::write(hostnames_dir.join(name), address).await {
        warn!("Failed to write tor-hostname copy for {}: {}", name, e);
    }
}

pub(super) async fn sync_all_hostname_copies(config: &ServicesConfig) {
    for svc in &config.services {
        if !svc.enabled {
            continue;
        }
        if let Some(addr) = read_onion_address(&svc.name).await {
            sync_single_hostname(&svc.name, &addr).await;
        }
    }
}

// ─── Service Listing ─────────────────────────────────────────────

/// Which packages the node knows about and which are installed — used to
/// hide hidden services for apps that aren't installed. ISO first-boot used
/// to pre-bake onions for a fixed app list (bitcoin/electrumx/lnd/btcpay/
/// mempool/fedimint), so fresh nodes showed Tor sites for apps that were
/// never installed (issue #79).
pub(super) struct AppInstallState {
    pub known: std::collections::HashSet<String>,
    pub installed: std::collections::HashSet<String>,
}

/// Package ids a Tor service name may correspond to. Service names predate
/// the catalog app ids (the ISO baked "bitcoin"/"btcpay"), so one service
/// can map to several package ids.
fn service_alias_candidates(name: &str) -> Vec<&str> {
    match name {
        "bitcoin" | "bitcoin-knots" | "bitcoin-core" => {
            vec!["bitcoin", "bitcoin-knots", "bitcoin-core"]
        }
        "electrumx" | "electrs" | "mempool-electrs" => {
            vec!["electrumx", "electrs", "mempool-electrs"]
        }
        "btcpay" | "btcpay-server" | "btcpayserver" => {
            vec!["btcpay", "btcpay-server", "btcpayserver"]
        }
        "mempool" | "mempool-web" => vec!["mempool", "mempool-web"],
        other => vec![other],
    }
}

impl AppInstallState {
    /// A service is listed unless it names a known-but-uninstalled app.
    /// The node's own service, the content relay, and custom user-created
    /// services (names matching no catalog package) always show.
    fn service_visible(&self, name: &str) -> bool {
        if name == "archipelago" || name == "relay" {
            return true;
        }
        let candidates = service_alias_candidates(name);
        if !candidates.iter().any(|c| self.known.contains(*c)) {
            return true; // not an app — custom hidden service
        }
        candidates.iter().any(|c| self.installed.contains(*c))
    }
}

pub(super) async fn list_services(
    config_dir: &std::path::Path,
    apps: Option<&AppInstallState>,
) -> Result<Vec<TorService>> {
    let base = detect_hidden_service_base();
    let config = load_services_config(config_dir).await;
    let mut services = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let visible = |name: &str| apps.map(|a| a.service_visible(name)).unwrap_or(true);

    for entry in &config.services {
        seen.insert(entry.name.clone());
        if !visible(&entry.name) {
            continue;
        }
        let onion = read_onion_address(&entry.name).await;
        services.push(TorService {
            name: entry.name.clone(),
            local_port: entry.local_port,
            onion_address: onion,
            enabled: entry.enabled,
            unauthenticated: entry.unauthenticated,
            protocol: is_protocol_service(&entry.name),
        });
    }

    for scan_dir in ["/var/lib/tor", &base] {
        if let Ok(mut entries) = tokio::fs::read_dir(scan_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                if name.starts_with("hidden_service_") && !name.contains("_old_") && is_dir {
                    let service_name = name
                        .strip_prefix("hidden_service_")
                        .unwrap_or(&name)
                        .to_string();
                    if seen.contains(&service_name) {
                        continue;
                    }
                    seen.insert(service_name.clone());
                    if !visible(&service_name) {
                        continue;
                    }
                    let onion = read_onion_address(&service_name).await;
                    let port = known_service_port(&service_name);
                    let is_proto = is_protocol_service(&service_name);
                    services.push(TorService {
                        name: service_name,
                        local_port: port,
                        onion_address: onion,
                        enabled: true,
                        unauthenticated: is_proto,
                        protocol: is_proto,
                    });
                }
            }
        }
    }

    Ok(services)
}

// ─── Onion Address Reading ───────────────────────────────────────

pub(super) async fn read_onion_address(service_name: &str) -> Option<String> {
    let hostnames_path = Path::new("/var/lib/archipelago/tor-hostnames").join(service_name);
    if let Some(addr) = read_and_validate_onion(&hostnames_path).await {
        return Some(addr);
    }

    let base = tor_data_dir();
    for search_base in &["/var/lib/tor", base.as_str()] {
        let path = Path::new(search_base)
            .join(format!("hidden_service_{}", service_name))
            .join("hostname");

        if let Some(addr) = read_and_validate_onion(&path).await {
            return Some(addr);
        }

        if let Some(addr) = sudo_read_and_validate_onion(&path).await {
            return Some(addr);
        }
    }

    None
}

async fn read_and_validate_onion(path: &Path) -> Option<String> {
    tokio::fs::read_to_string(path)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| is_valid_v3_onion(s))
}

async fn sudo_read_and_validate_onion(path: &Path) -> Option<String> {
    tokio::process::Command::new("sudo")
        .args(["cat", &path.to_string_lossy()])
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| is_valid_v3_onion(s))
}

fn is_valid_v3_onion(s: &str) -> bool {
    s.len() == 62
        && s.ends_with(".onion")
        && !s.contains(':')
        && s[..56].chars().all(|c| c.is_ascii_alphanumeric())
}

// ─── Known Ports ─────────────────────────────────────────────────

pub(in crate::api::rpc) fn known_service_port(name: &str) -> u16 {
    match name {
        "archipelago" => 80,
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => 8333,
        "electrs" | "electrumx" => 50001,
        "lnd" => 8080,
        "btcpay" | "btcpay-server" | "btcpayserver" => 23000,
        "mempool" => 4080,
        "fedimint" => 8175,
        "nostr-relay" | "nostr-rs-relay" => 8080,
        "searxng" => 8888,
        "ollama" => 11434,
        "filebrowser" => 8083,
        "grafana" => 3000,
        "home-assistant" => 8123,
        "immich" => 2283,
        "photoprism" => 2342,
        "penpot" => 9001,
        "nginx-proxy-manager" => 8081,
        "vaultwarden" => 8343,
        "indeedhub" => 7778,
        _ => 0,
    }
}

pub(in crate::api::rpc) fn is_protocol_service(name: &str) -> bool {
    matches!(
        name,
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" | "electrs" | "electrumx" | "lnd"
    )
}

/// Apps that must never gain a Tor hidden service *automatically*.
///
/// These are credential-bearing reverse proxies in front of the node's money:
/// `lnd-ui` (host port 18083) serves `/lnd-connect-info` and fronts LND, and
/// `bitcoin-ui` (host port 8334) proxies Bitcoin Core RPC with the RPC
/// password injected on the caller's behalf. Both are session-gated at the
/// backend as of the 2026-08-02 fix, but an onion is a *global, persistent*
/// door, and neither should acquire one merely as a side effect of being
/// installed.
///
/// [`is_protocol_service`] does not cover them: it names the daemons (`lnd`,
/// `bitcoin-knots`), not their UI sidecars. Without this list,
/// `auto_add_tor_service` would publish `onion:80 -> 127.0.0.1:18083` and
/// `-> :8334` the moment either app installs — which is exactly the surface
/// that leaked the LND admin macaroon, re-exposed worldwide instead of just
/// to mesh/LAN peers.
///
/// This gates only the automatic path. An operator who deliberately enables
/// Tor for one of these apps still can; that is an informed choice, not a
/// silent default.
pub(in crate::api::rpc) fn never_auto_onioned(name: &str) -> bool {
    matches!(name, "lnd-ui" | "bitcoin-ui")
}

// ─── Config I/O ──────────────────────────────────────────────────

fn tor_data_dir() -> String {
    std::env::var("TOR_DATA_DIR").unwrap_or_else(|_| TOR_DATA_DIR.to_string())
}

pub(in crate::api::rpc) async fn load_services_config(
    config_dir: &std::path::Path,
) -> ServicesConfig {
    let path = config_dir.join(SERVICES_CONFIG);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => ServicesConfig::default(),
    }
}

pub(in crate::api::rpc) async fn save_services_config(
    config_dir: &std::path::Path,
    config: &ServicesConfig,
) -> Result<()> {
    tokio::fs::create_dir_all(config_dir)
        .await
        .context("Failed to create tor config dir")?;
    let path = config_dir.join(SERVICES_CONFIG);
    let content =
        serde_json::to_string_pretty(config).context("Failed to serialize services config")?;
    tokio::fs::write(&path, content)
        .await
        .context("Failed to write services config")?;
    Ok(())
}

// ─── Federation Notification ─────────────────────────────────────

pub(super) async fn notify_federation_peers_address_change(
    data_dir: &std::path::Path,
    new_onion: &str,
    old_onion: Option<&str>,
    tor_proxy: Option<&str>,
) {
    let identity_dir = data_dir.join("identity");
    match identity::NodeIdentity::load_or_create(&identity_dir).await {
        Ok(node_id) => {
            let did = match node_id.did_key() {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("Failed to derive DID key: {}", e);
                    return;
                }
            };
            // `tor_proxy` is retained for API compat but unused — the FIPS
            // fallback dial uses constants::TOR_SOCKS_PROXY internally.
            let _ = tor_proxy;
            match federation::load_nodes(data_dir).await {
                Ok(peers) => {
                    for peer in peers {
                        if peer.onion.is_empty() && peer.fips_npub.is_none() {
                            continue;
                        }
                        let payload = serde_json::json!({
                            "method": "federation.peer-address-changed",
                            "params": {
                                "did": did,
                                "new_onion": new_onion,
                                "old_onion": old_onion,
                            }
                        });
                        // FIPS-preferred: peer's fips_npub is stable across
                        // onion rotation, so this notification reaches them
                        // even when their (or our) old onion is now stale.
                        let req = crate::fips::dial::PeerRequest::new(
                            peer.fips_npub.as_deref(),
                            &peer.onion,
                            "/rpc/v1",
                        )
                        .service(crate::settings::transport::PeerService::Peers)
                        .timeout(std::time::Duration::from_secs(30))
                        .fips_timeout(std::time::Duration::from_secs(6))
                        .record_transport(data_dir);
                        match req.send_json(&payload).await {
                            Ok((_, transport)) => {
                                info!(peer_did = %peer.did, transport = %transport, "Notified peer of address change")
                            }
                            Err(e) => warn!(peer_did = %peer.did, "Failed to notify peer: {}", e),
                        }
                    }
                }
                Err(e) => warn!("Failed to load federation peers: {}", e),
            }
        }
        Err(e) => warn!("Failed to load node identity for propagation: {}", e),
    }
}

// ─── Hostname Waiting ────────────────────────────────────────────

pub(in crate::api::rpc) async fn wait_for_hostname(
    service_name: &str,
    max_secs: u64,
) -> Option<String> {
    for _ in 0..max_secs {
        if let Some(addr) = read_onion_address(service_name).await {
            return Some(addr);
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    warn!(
        service = service_name,
        "Timed out waiting for new .onion hostname"
    );
    None
}
