use super::RpcHandler;
use crate::network::router as net_router;
use crate::tollgate_installer::InstallerClient;
use anyhow::{Context, Result};
use archipelago_openwrt::{
    detect,
    opkg::PkgManager,
    router::Router,
    tollgate::{self, version::Installed, TollGateConfig},
    wan, wifi_scan,
};
use serde::Deserialize;
use std::time::Duration;

/// Default port for the local Cashu mint (nutshell / cashu-mint app).
const LOCAL_MINT_PORT: u16 = 3338;

/// Run one full provision pass over a fresh SSH session.
///
/// Kept as a free function so the retry below can run the same pass twice
/// without cloning the handler or moving the credential plumbing into a
/// closure.
async fn provision_tollgate_once(
    host: String,
    ssh_user: String,
    ssh_password: String,
    config: TollGateConfig,
) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
        router.verify_openwrt()?;
        tokio::runtime::Handle::current().block_on(tollgate::provision_native(&router, &config))
    })
    .await
    .context("openwrt provision task")?
}

/// Run only the Archy-owned post-install phase over SSH. Used after the
/// official installer has flashed/installed the package and verified it.
async fn provision_after_install_once(
    host: String,
    ssh_user: String,
    ssh_password: String,
    config: TollGateConfig,
) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
        router.verify_openwrt()?;
        // The installer owns the device code. Derive the product names from
        // it so a re-deploy keeps the same `archipelago-tollgate-XXXX` and
        // `noderunner-XXXX`, regardless of which SSID the released installer
        // wrote first. An operator-supplied custom private SSID is preserved.
        let mut config = config;
        if let Ok(code) = router.uci_get("tollgate.device.code") {
            let code = code.trim().to_ascii_uppercase();
            if !code.is_empty() {
                config.ssid = format!("archipelago-tollgate-{code}");
                if config.private_ssid.is_empty()
                    || config.private_ssid == "noderunner"
                    || config.private_ssid == "noderunner-private"
                {
                    config.private_ssid = format!("noderunner-{code}");
                }
            }
        }
        tokio::runtime::Handle::current()
            .block_on(tollgate::provision_after_install(&router, &config))
    })
    .await
    .context("openwrt post-install task")?
}

/// Whether the router reports the pinned TollGate package installed.
///
/// Used only after a provision failed: the package's post-install script
/// restarts the network, which kills the SSH session that was running `apk
/// add`. The package is already on disk when that happens, so "the session
/// died" is not evidence that the install failed.
async fn tollgate_is_at_pin(
    host: String,
    ssh_user: String,
    ssh_password: String,
    desired: String,
) -> bool {
    tokio::task::spawn_blocking(move || -> Result<bool> {
        let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
        Ok(tollgate::installed_tollgate_version_any(&router).satisfies(&desired))
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or(false)
}

/// Quote one argument for the router's POSIX shell.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// How the node reaches a candidate: `(interface, "wifi"|"ethernet", subnet)`.
/// Best-effort; unknown values are empty strings.
async fn link_context(ip: &str) -> (String, String, String) {
    let route = tokio::process::Command::new("ip")
        .args(["route", "get", ip])
        .output()
        .await;
    let Ok(route) = route else {
        return (String::new(), String::new(), String::new());
    };
    if !route.status.success() {
        return (String::new(), String::new(), String::new());
    }
    let route_text = String::from_utf8_lossy(&route.stdout);
    let iface = route_text
        .split_whitespace()
        .skip_while(|word| *word != "dev")
        .nth(1)
        .unwrap_or("")
        .to_string();
    if iface.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    let link = if std::path::Path::new(&format!("/sys/class/net/{iface}/wireless")).exists() {
        "wifi"
    } else {
        "ethernet"
    };
    let addr = tokio::process::Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", &iface])
        .output()
        .await;
    let subnet = addr
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            let cidr = text
                .split_whitespace()
                .skip_while(|word| *word != "inet")
                .nth(1)?
                .to_string();
            let (ip, prefix) = cidr.split_once('/')?;
            let octets: Vec<&str> = ip.split('.').collect();
            if octets.len() != 4 {
                return None;
            }
            Some(format!(
                "{}.{}.{}.0/{}",
                octets[0], octets[1], octets[2], prefix
            ))
        })
        .unwrap_or_default();
    (iface, link.to_string(), subnet)
}

/// Fetch a URL from the router using whichever HTTP client is installed.
///
/// OpenWrt images are not uniform: some ship `curl`, some only
/// `uclient-fetch`, and a stock BusyBox image only has `wget`. The old code
/// called `curl` unconditionally, so every advertisement and mint read failed
/// on 25.12 with `sh: curl: not found`. The wrapper prints the first
/// successful body, or one `__ERROR__:` line after every client failed; it
/// never mixes a failed client's stderr into a later successful body.
fn router_http_get(router: &Router, url: &str, timeout_secs: u32) -> Result<String> {
    let script = format!(
        "out=$(curl -fsS -m {timeout_secs} '{url}' 2>&1) && {{ printf '%s' \"$out\"; exit 0; }}; \
         out=$(uclient-fetch -q -O - -T {timeout_secs} '{url}' 2>&1) && {{ printf '%s' \"$out\"; exit 0; }}; \
         out=$(wget -q -O - -T {timeout_secs} '{url}' 2>&1) && {{ printf '%s' \"$out\"; exit 0; }}; \
         printf '__ERROR__:%s' \"$out\"; exit 0"
    );
    let out = router.run_ok(&script)?;
    if let Some(err) = out.strip_prefix("__ERROR__:") {
        anyhow::bail!("HTTP request failed: {}", err.trim());
    }
    Ok(out)
}

/// Compare mint URLs the way the daemon does: a trailing slash is not part of
/// the mint identity, and neither is the host's case.
fn mint_url_key(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

#[derive(Debug, Clone, Default)]
struct DaemonStatus {
    running: bool,
    version: String,
    uptime: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CliStatusEnvelope {
    success: bool,
    #[serde(default)]
    data: Option<CliStatusData>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CliStatusData {
    #[serde(default)]
    running: bool,
    #[serde(default)]
    version: String,
    #[serde(default)]
    uptime: String,
}

/// Parse `tollgate --json status`. The daemon reports its own version (e.g.
/// `TollGate v0.6.0-g9364f2e`), which is not the same string as the installed
/// package (`0.6.0-r1`); the caller normalises both.
fn parse_daemon_status(out: &str) -> Option<DaemonStatus> {
    let envelope: CliStatusEnvelope = serde_json::from_str(out).ok()?;
    if envelope.success {
        let data = envelope.data?;
        Some(DaemonStatus {
            running: data.running,
            version: data.version,
            uptime: data.uptime,
            error: None,
        })
    } else {
        Some(DaemonStatus {
            running: false,
            version: String::new(),
            uptime: String::new(),
            error: envelope.error,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct AdvertisedMint {
    url: String,
    price: u64,
    unit: String,
    min_steps: u64,
}

/// Parse the `price_per_step` tags out of the daemon's kind-10021
/// advertisement. This is the list the captive portal renders, so it is the
/// only honest way to answer "what is the router actually selling?".
fn parse_advertisement(out: &str) -> Vec<AdvertisedMint> {
    let Ok(event) = serde_json::from_str::<serde_json::Value>(out) else {
        return Vec::new();
    };
    let Some(tags) = event.get("tags").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    tags.iter()
        .filter_map(|tag| {
            let tag = tag.as_array()?;
            if tag.first()?.as_str()? != "price_per_step" {
                return None;
            }
            Some(AdvertisedMint {
                url: tag.get(4)?.as_str()?.to_string(),
                price: tag.get(2)?.as_str()?.parse().ok()?,
                unit: tag
                    .get(3)
                    .and_then(|v| v.as_str())
                    .unwrap_or("sat")
                    .to_string(),
                min_steps: tag
                    .get(5)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            })
        })
        .collect()
}

/// Compare the daemon's self-reported version with the pinned/package version.
///
/// The daemon prints `TollGate v0.6.0-g9364f2e`; the package reports
/// `0.6.0-r1`. Strip the product name, the leading `v`, and any `-g<sha>`
/// suffix before comparing.
fn daemon_version_matches(daemon: &str, desired: &str) -> bool {
    let daemon = daemon.trim();
    let daemon = daemon.strip_prefix("TollGate").unwrap_or(daemon).trim();
    let daemon = daemon.strip_prefix('v').unwrap_or(daemon);
    let core = daemon.split(['-', '+']).next().unwrap_or(daemon);
    tollgate::version::normalise_version(core) == tollgate::version::normalise_version(desired)
}

/// Reject a mint URL before it reaches a shell command. The router-side probe
/// interpolates the URL into `curl`, so quotes/whitespace/backslashes are
/// refused rather than escaped.
fn validate_mint_url(url: &str) -> Result<()> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        anyhow::bail!("mint URL must start with http:// or https://: {url}");
    }
    if url
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\'))
    {
        anyhow::bail!("mint URL contains an unsafe character: {url}");
    }
    Ok(())
}

/// Prefer the entry that represents the router the operator is actually
/// managing: active config first, then a remembered credential, then a
/// completed identify.
fn router_score(v: &serde_json::Value) -> i32 {
    let mut score = 0;
    if v.get("active").and_then(|x| x.as_bool()).unwrap_or(false) {
        score += 100;
    }
    if v.get("has_saved_credentials")
        .and_then(|x| x.as_bool())
        .unwrap_or(false)
    {
        score += 50;
    }
    if v.get("password_status").and_then(|x| x.as_str()) == Some("accepted") {
        score += 20;
    }
    if v.get("identified")
        .and_then(|x| x.as_bool())
        .unwrap_or(false)
    {
        score += 10;
    }
    if v.get("ssh_open").and_then(|x| x.as_bool()).unwrap_or(false) {
        score += 5;
    }
    score
}

/// Fill keys the winner does not have (or has as null) from the duplicate.
fn merge_missing(dst: &mut serde_json::Value, src: &serde_json::Value) {
    let (Some(dst), Some(src)) = (dst.as_object_mut(), src.as_object()) else {
        return;
    };
    for (key, value) in src {
        // Trust/auth state is per address, not per physical router. The
        // duplicate's fingerprint must not reappear after the selected
        // address was trusted.
        if matches!(
            key.as_str(),
            "ip" | "mac"
                | "active"
                | "has_saved_credentials"
                | "ssh_fingerprint"
                | "password_status"
                | "source"
                | "note"
        ) {
            continue;
        }
        let missing = !dst.contains_key(key) || dst.get(key).map(|v| v.is_null()).unwrap_or(false);
        if missing {
            dst.insert(key.clone(), value.clone());
        }
    }
}

/// Resolve the 4-character device code the installer/module store, falling
/// back to a machine-shaped hostname suffix and then the MAC.
fn device_code(router: &Router) -> String {
    let is_code = |s: &str| {
        s.len() == 4
            && s.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    };
    let stored = router
        .uci_get("tollgate.device.code")
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    if is_code(&stored) {
        return stored;
    }
    let hostname = router
        .uci_get("system.@system[0].hostname")
        .unwrap_or_default();
    let suffix = hostname
        .trim()
        .rsplit('-')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    if is_code(&suffix) {
        return suffix;
    }
    let mac = router
        .run_ok(
            "cat /sys/class/net/br-lan/address 2>/dev/null || \
             cat /sys/class/net/eth0/address 2>/dev/null",
        )
        .unwrap_or_default();
    let hex: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() >= 4 {
        return hex[hex.len() - 4..].to_ascii_uppercase();
    }
    String::new()
}

/// Probe one Cashu mint from the router and return the same shape the UI's
/// per-mint "Test" button consumes. `/v1/info` is best-effort; `/v1/keysets`
/// is the authoritative reachability/active-sat-keyset check.
fn probe_mint_with_router(router: &Router, url: &str) -> Result<serde_json::Value> {
    let base = url.trim_end_matches('/');
    let info_out = router_http_get(router, &format!("{base}/v1/info"), 10).unwrap_or_default();
    let info: serde_json::Value =
        serde_json::from_str(&info_out).unwrap_or_else(|_| serde_json::json!({}));
    let keys_out = router_http_get(router, &format!("{base}/v1/keysets"), 10)
        .with_context(|| format!("probe {url} from the router"))?;
    let keys: serde_json::Value =
        serde_json::from_str(&keys_out).with_context(|| format!("parse {url}/v1/keysets"))?;
    let active = keys
        .get("keysets")
        .and_then(|v| v.as_array())
        .and_then(|a| {
            a.iter().find(|k| {
                k.get("unit").and_then(|v| v.as_str()) == Some("sat")
                    && k.get("active").and_then(|v| v.as_bool()).unwrap_or(false)
            })
        });
    let active_sat = active.is_some();
    let fee = active
        .and_then(|k| k.get("input_fee_ppk"))
        .and_then(|v| v.as_u64());
    Ok(serde_json::json!({
        "url": url,
        "ok": active_sat,
        "name": info.get("name").cloned().unwrap_or_default(),
        "version": info.get("version").cloned().unwrap_or_default(),
        "active_sat_keyset": active_sat,
        "input_fee_ppk": fee,
    }))
}

impl RpcHandler {
    /// Scan the local subnet for OpenWrt routers.
    ///
    /// Params: `{ "subnet": "192.168.1.0", "prefix": 24,
    ///             "ssh_user": "root", "ssh_password": "" }`
    pub(super) async fn handle_openwrt_scan(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let p = params.unwrap_or_default();
        let subnet: [u8; 4] = parse_ipv4(
            p.get("subnet")
                .and_then(|v| v.as_str())
                .unwrap_or("192.168.1.0"),
        )?;
        let prefix = p.get("prefix").and_then(|v| v.as_u64()).unwrap_or(24) as u8;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .unwrap_or("root")
            .to_string();
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // scan_subnet is `async` in name only: up to 255 SEQUENTIAL blocking
        // TCP probes at 500ms each (~2 min on a /24 that silently drops),
        // plus a blocking SSH verify per candidate. Inline, one click of
        // "scan for routers" held a tokio worker for that whole time.
        let routers = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(detect::scan_subnet(
                subnet,
                prefix,
                &ssh_user,
                &ssh_password,
            ))
        })
        .await
        .context("openwrt scan task")?;
        let ips: Vec<String> = routers.iter().map(|ip| ip.to_string()).collect();

        Ok(serde_json::json!({ "routers": ips }))
    }

    /// Read current settings from a saved or ad-hoc OpenWrt router via SSH/UCI.
    ///
    /// Params (all optional): `{ "host": "...", "ssh_user": "root", "ssh_password": "" }`
    /// If params are omitted the saved `router_config.json` credentials are used.
    pub(super) async fn handle_openwrt_get_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();
        let host_from_params = p.get("host").and_then(|v| v.as_str()).is_some();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;

        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());

        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        // The SSH session is blocking (ssh2 over std TcpStream). Run it on the
        // blocking pool: inline it used to park a tokio worker for the whole
        // exchange, and against an unreachable router (the gateway that stayed
        // behind after a node moved networks) the periodic dashboard poll
        // stalled unrelated RPCs for tens of seconds — long enough that TOTP
        // codes expired in flight (framework-pt, 2026-08-15).
        let status = {
            let host = host.clone();
            let ssh_user = ssh_user.clone();
            let ssh_password = ssh_password.clone();
            // The official installer owns the release tag. Read it without
            // starting the service (a status poll must not spawn processes),
            // with a short timeout so an absent installer cannot stall the
            // dashboard.
            let installer_release = match tokio::time::timeout(
                Duration::from_secs(1),
                InstallerClient::new(&self.config.data_dir).config(),
            )
            .await
            {
                Ok(Ok(cfg)) => cfg
                    .get("feed_release_tag")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                _ => None,
            };

            tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
                let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
                router.verify_openwrt()?;

                // System info
                let release = router
                    .run_ok("cat /etc/openwrt_release")
                    .unwrap_or_default();
                let hostname = router
                    .uci_get("system.@system[0].hostname")
                    .unwrap_or_else(|_| "unknown".into());
                let uptime_secs: u64 = router
                    .run_ok("cat /proc/uptime")
                    .unwrap_or_default()
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.split('.').next())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);

                // TollGate — presence AND version.
                //
                // This probe used to be presence-only, and it used the
                // repository name (`tollgate-module-basic-go`) as if it were a
                // package name. The package is `tollgate-wrt`, so that grep
                // could never match and every bit of information came from the
                // file-existence fallback — which says nothing about version.
                //
                // The consequence was that a version bump was invisible: the
                // dashboard reported "installed" identically for 0.5.0 and
                // 0.6.0, so the one question this release has to answer —
                // "did the pin actually take effect?" — could not be asked.
                // Detect the manager for the *reported* value only. The version
                // read must not depend on that detection succeeding: guessing a
                // manager here would run its listing on a router that may not
                // have it, which fails, parses as `Absent`, and reports a
                // healthy router as "not installed". Ask both listings instead.
                let pkg_mgr = router.pkg_manager().ok();
                let installed_version = tollgate::installed_tollgate_version_any(&router);
                let tollgate_installed = !matches!(installed_version, Installed::Absent);

                let tollgate = if tollgate_installed {
                    // Whether the installed build matches the version this
                    // binary would install. Reported as its own field rather
                    // than folded into `installed`, so "wrong version" is
                    // visible instead of looking like success.
                    let desired = installer_release
                        .as_deref()
                        .and_then(|tag| tollgate::package_version_from_pin(tag).ok())
                        .unwrap_or_else(|| {
                            tollgate::TOLLGATE_VERSION.trim_start_matches('v').to_string()
                        });
                    let version_str = match &installed_version {
                        Installed::Version(v) => v.clone(),
                        Installed::UnknownVersion => String::new(),
                        Installed::Absent => String::new(),
                    };
                    let up_to_date = installed_version.satisfies(&desired);

                    // The package version says what is on disk. It does not
                    // say what is running: a v0.6.0 upgrade can leave the old
                    // daemon holding :2121, still advertising its factory
                    // mint list while the new config sits unread on disk.
                    let daemon = router
                        .run_ok("tollgate --json status 2>/dev/null")
                        .ok()
                        .and_then(|out| parse_daemon_status(&out));
                    let (daemon_running, daemon_version, daemon_uptime, daemon_error) = match &daemon
                    {
                        Some(d) => (
                            d.running,
                            d.version.clone(),
                            d.uptime.clone(),
                            d.error.clone(),
                        ),
                        None => (
                            false,
                            String::new(),
                            String::new(),
                            Some(
                                "tollgate --json status did not return a usable response".to_string(),
                            ),
                        ),
                    };
                    let daemon_matches =
                        daemon_running && daemon_version_matches(&daemon_version, &desired);
                    let advertised_mints =
                        router_http_get(&router, "http://127.0.0.1:2121/", 5)
                            .ok()
                            .map(|out| parse_advertisement(&out))
                            .unwrap_or_default();
                    let configured_mints: Vec<tollgate::MintConfig> = router
                        .run_ok("cat /etc/tollgate/config.json 2>/dev/null")
                        .ok()
                        .and_then(|out| serde_json::from_str::<serde_json::Value>(&out).ok())
                        .and_then(|doc| doc.get("accepted_mints").cloned())
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default();
                    let configured_mint_count = configured_mints.len();

                    serde_json::json!({
                        "installed": true,
                        "version": version_str,
                        "version_known": matches!(installed_version, Installed::Version(_)),
                        "pinned_version": desired,
                        "installer_feed_tag": installer_release.clone(),
                        "up_to_date": up_to_date,
                        "daemon_running": daemon_running,
                        "daemon_version": daemon_version,
                        "daemon_uptime": daemon_uptime,
                        "daemon_error": daemon_error,
                        "daemon_version_matches": daemon_matches,
                        "advertised_mints": advertised_mints,
                        "configured_mints": configured_mint_count,
                        "accepted_mints": configured_mints,
                        "pkg_manager": pkg_mgr.map(|m| match m {
                            PkgManager::Opkg => "opkg",
                            PkgManager::ApkNative => "apk",
                        }),
                        "enabled":       router.uci_get("tollgate.main.enabled").map(|v| v == "1").unwrap_or(false),
                        "metric":        router.uci_get("tollgate.main.metric").unwrap_or_default(),
                        "step_size_ms":  router.uci_get("tollgate.main.step_size").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
                        "price_per_step":router.uci_get("tollgate.main.price_per_step").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
                        "min_steps":     router.uci_get("tollgate.main.min_steps").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1),
                        "currency":      router.uci_get("tollgate.main.currency").unwrap_or_default(),
                        "mint_url":      router.uci_get("tollgate.main.mint_url").unwrap_or_default(),
                        "payout_address":router.uci_get("tollgate.main.payout_address").unwrap_or_default(),
                        "private_ssid":  router.uci_get("tollgate.main.private_ssid").unwrap_or_default(),
                    })
                } else {
                    serde_json::json!({
                        "installed": false,
                        "version": "",
                        "version_known": false,
                        "pinned_version": tollgate::TOLLGATE_VERSION.trim_start_matches('v'),
                        "up_to_date": false,
                    })
                };

                // WiFi interfaces
                let wifi_raw = router.run_ok("uci show wireless").unwrap_or_default();
                let wifi_interfaces = parse_wifi_interfaces(&wifi_raw);

                let wan_status = wan::get_wan_status(&router);

                Ok(serde_json::json!({
                    "host": host,
                    "hostname": hostname,
                    "uptime_secs": uptime_secs,
                    "release": parse_release(&release),
                    "tollgate": tollgate,
                    "wifi_interfaces": wifi_interfaces,
                    "wan": wan_status,
                }))
            })
            .await
            .context("openwrt status task")??
        };

        // Persist the connection so other views (e.g. the Home dashboard's
        // Network tile) can poll `openwrt.get-status` with no params instead
        // of every caller needing to carry host/credentials around. Only do
        // this when the host actually came from params — otherwise every
        // no-args poll would re-save the same thing it just read.
        if host_from_params {
            let _ = net_router::configure_router(
                &self.config.data_dir,
                net_router::RouterType::OpenWrt,
                &host,
                None,
                Some(&ssh_user),
                Some(&ssh_password),
            )
            .await;
        }

        Ok(status)
    }

    /// Probe a Cashu mint from the router.
    ///
    /// Params: host/credentials + `{ "url": "https://..." }`.
    /// The probe is deliberately router-side: a mint reachable from the node
    /// but not from the router is unusable for the captive portal.
    pub(super) async fn handle_openwrt_test_mint(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!("No router configured — provide host or configure the router first")
            })?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();
        let url = p
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("url is required"))?
            .to_string();
        validate_mint_url(&url)?;

        let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
            let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
            router.verify_openwrt()?;
            probe_mint_with_router(&router, &url)
        })
        .await
        .context("openwrt mint test task")??;

        Ok(result)
    }

    /// Compare what the daemon advertises with what the config accepts, and
    /// probe the missing mints from the router.
    ///
    /// This is deliberately separate from the wallet: an empty wallet is not
    /// a degraded mint, and a count mismatch is not evidence that the daemon
    /// needs a restart.
    pub(super) async fn handle_openwrt_mint_health(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();
        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No router configured"))?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
            let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
            router.verify_openwrt()?;
            let configured: Vec<tollgate::MintConfig> = router
                .run_ok("cat /etc/tollgate/config.json 2>/dev/null")
                .ok()
                .and_then(|out| serde_json::from_str::<serde_json::Value>(&out).ok())
                .and_then(|doc| doc.get("accepted_mints").cloned())
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let advertised = router_http_get(&router, "http://127.0.0.1:2121/", 5)
                .ok()
                .map(|out| parse_advertisement(&out))
                .unwrap_or_default();

            let mut mints = Vec::new();
            for mint in &configured {
                if mint.url.trim().is_empty() {
                    continue;
                }
                let key = mint_url_key(&mint.url);
                let is_advertised = advertised.iter().any(|a| mint_url_key(&a.url) == key);
                let (ok, error) = if is_advertised {
                    (true, String::new())
                } else {
                    match probe_mint_with_router(&router, &mint.url) {
                        Ok(probe) => {
                            let ok = probe.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                            let error = if ok {
                                "reachable but not advertised by the daemon".to_string()
                            } else {
                                "no active sat keyset".to_string()
                            };
                            (ok, error)
                        }
                        Err(err) => (false, err.to_string()),
                    }
                };
                mints.push(serde_json::json!({
                    "url": mint.url,
                    "advertised": is_advertised,
                    "ok": ok,
                    "error": error,
                }));
            }

            Ok(serde_json::json!({
                "configured": configured.len(),
                "advertised": advertised.len(),
                "mints": mints,
            }))
        })
        .await
        .context("openwrt mint health task")??;

        Ok(result)
    }

    /// Discover every router the node can manage on its interfaces.
    ///
    /// The installer already scans the host's LAN interfaces and returns
    /// model/vendor/firmware/trust metadata; this wraps that list, marks the
    /// saved active router, and leaves selection explicit.
    /// Run one `tollgate --json <args>` against the active router and return
    /// its `data` payload. Used by the admin wallet/upstream panels.
    async fn tollgate_json(&self, args: &str) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        if !saved.configured {
            anyhow::bail!("No active router is selected");
        }
        let host = saved.address;
        let user = saved.username.unwrap_or_else(|| "root".to_string());
        let pass = saved.password.unwrap_or_default();
        let command = format!("tollgate --json {args} 2>/dev/null");
        let out = tokio::task::spawn_blocking(move || -> Result<String> {
            let router = Router::connect_password(&host, 22, &user, &pass)?;
            router.verify_openwrt()?;
            router.run_ok(&command)
        })
        .await
        .context("tollgate admin command task")??;
        let value: serde_json::Value =
            serde_json::from_str(&out).context("parse tollgate JSON output")?;
        if value.get("success").and_then(|v| v.as_bool()) == Some(false) {
            let error = value
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("tollgate command failed");
            anyhow::bail!("{error}");
        }
        Ok(value
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Wallet total and per-mint balances. A degraded wallet is returned as a
    /// structured failure (so the UI can show the reason) rather than an RPC
    /// error.
    pub(super) async fn handle_openwrt_wallet_info(&self) -> Result<serde_json::Value> {
        match self.tollgate_json("wallet info").await {
            Ok(wallet) => Ok(serde_json::json!({
                "ok": true,
                "degraded": false,
                "wallet": wallet,
            })),
            Err(err) => Ok(serde_json::json!({
                "ok": false,
                "degraded": true,
                "error": err.to_string(),
            })),
        }
    }

    pub(super) async fn handle_openwrt_upstream_list(&self) -> Result<serde_json::Value> {
        // The user-facing CLI subcommand is `list`; its server-side action is
        // `list-upstream`. Calling the server action through the CLI is a
        // usage error, so use the command the router actually exposes.
        Ok(serde_json::json!({
            "upstreams": self.tollgate_json("upstream list").await?,
        }))
    }

    pub(super) async fn handle_openwrt_upstream_known(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "known": self.tollgate_json("upstream known").await?,
        }))
    }

    pub(super) async fn handle_openwrt_upstream_scan(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "networks": self.tollgate_json("upstream scan").await?,
        }))
    }

    pub(super) async fn handle_openwrt_upstream_connect(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let p = params.unwrap_or_default();
        let ssid = p
            .get("ssid")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("ssid is required"))?;
        let passphrase = p.get("passphrase").and_then(|v| v.as_str()).unwrap_or("");
        let mut args = format!("upstream connect {}", shell_quote(ssid));
        if !passphrase.is_empty() {
            args.push(' ');
            args.push_str(&shell_quote(passphrase));
        }
        let result = self.tollgate_json(&args).await?;
        Ok(serde_json::json!({ "ok": true, "result": result }))
    }

    pub(super) async fn handle_openwrt_upstream_remove(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let p = params.unwrap_or_default();
        let ssid = p
            .get("ssid")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("ssid is required"))?;
        let result = self
            .tollgate_json(&format!("upstream remove {}", shell_quote(ssid)))
            .await?;
        Ok(serde_json::json!({ "ok": true, "result": result }))
    }

    /// Remove mints from `accepted_mints` without reinstalling TollGate or
    /// touching the wallet database.
    pub(super) async fn handle_openwrt_prune_mints(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();
        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.configured.then(|| saved.address.clone()))
            .ok_or_else(|| anyhow::anyhow!("No router configured"))?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();
        let urls: Vec<String> = p
            .get("urls")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if urls.is_empty() {
            anyhow::bail!("urls is required");
        }

        let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
            let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
            router.verify_openwrt()?;
            let remaining = tollgate::config::remove_mints(&router, &urls)?;
            let enabled = router
                .uci_get("tollgate.main.enabled")
                .map(|v| v == "1")
                .unwrap_or(true);
            if enabled {
                tollgate::restart_tollgate_daemon(&router)?;
            }
            Ok(serde_json::json!({
                "ok": true,
                "remaining": remaining,
            }))
        })
        .await
        .context("openwrt prune mints task")??;

        Ok(result)
    }

    /// Apply the product AP names (`archipelago-tollgate-<code>` and
    /// `noderunner-<code>`) to an already-provisioned router.
    pub(super) async fn handle_openwrt_apply_network_names(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();
        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.configured.then(|| saved.address.clone()))
            .ok_or_else(|| anyhow::anyhow!("No router configured"))?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
            let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
            router.verify_openwrt()?;
            let code = device_code(&router);
            if code.is_empty() {
                anyhow::bail!("could not resolve a 4-character device code");
            }
            let public_ssid = format!("archipelago-tollgate-{code}");
            let private_ssid = format!("noderunner-{code}");
            let enabled = router
                .uci_get("tollgate.main.enabled")
                .map(|v| v == "1")
                .unwrap_or(true);
            tollgate::rename_network(&router, &public_ssid, &private_ssid, enabled)?;
            Ok(serde_json::json!({
                "ok": true,
                "code": code,
                "public_ssid": public_ssid,
                "private_ssid": private_ssid,
            }))
        })
        .await
        .context("openwrt apply network names task")??;

        Ok(result)
    }

    pub(super) async fn handle_openwrt_discover(&self) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let known = net_router::load_known_routers(&self.config.data_dir).await?;
        let client = InstallerClient::new(&self.config.data_dir);
        let cfg = client
            .ensure_running()
            .await
            .context("start the local tollgate-installer")?;
        let scan = client.scan().await.context("scan the LAN")?;
        let mut routers = scan
            .get("routers")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for router in &mut routers {
            let ip = router
                .get("ip")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mac = router
                .get("mac")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let remembered = net_router::find_known_router(&known, &ip, mac.as_deref());
            let active_has_password = saved.configured
                && ip == saved.address
                && saved
                    .password
                    .as_deref()
                    .map(|p| !p.is_empty())
                    .unwrap_or(false);
            router["active"] = serde_json::json!(saved.configured && ip == saved.address);
            router["has_saved_credentials"] =
                serde_json::json!(remembered.is_some() || active_has_password);
            let (interface, link_type, subnet) = link_context(&ip).await;
            router["interface"] = serde_json::json!(interface);
            router["link_type"] = serde_json::json!(link_type);
            router["subnet"] = serde_json::json!(subnet);
            if let Some(remembered) = remembered {
                if !remembered.hostname.is_empty() {
                    router["saved_hostname"] = serde_json::json!(remembered.hostname);
                }
                if !remembered.model.is_empty() {
                    router["saved_model"] = serde_json::json!(remembered.model);
                }
            }
            let model = router
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let saved_model = router
                .get("saved_model")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let kind = if model.contains("gl.inet")
                || model.contains("gl-mt")
                || saved_model.contains("gl.inet")
                || saved_model.contains("gl-mt")
            {
                "gl-inet"
            } else if router
                .get("identified")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                "openwrt"
            } else if router
                .get("ssh_open")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                "ssh"
            } else {
                "unknown"
            };
            router["kind"] = serde_json::json!(kind);
        }

        // The installer scan lists one entry per management address, so a
        // router that answers on both its WAN and LAN appears twice. Keep one
        // row per MAC, preferring the active/saved-credential entry and
        // filling any fields the other address knew.
        let mut deduped: Vec<serde_json::Value> = Vec::new();
        for router in routers {
            let mac = router
                .get("mac")
                .and_then(|v| v.as_str())
                .map(|s| s.to_ascii_lowercase());
            let fingerprint = router
                .get("ssh_fingerprint")
                .and_then(|v| v.as_str())
                .map(|s| s.to_ascii_lowercase());
            if mac.is_some() || fingerprint.is_some() {
                if let Some(pos) = deduped.iter().position(|r| {
                    let other_mac = r
                        .get("mac")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_ascii_lowercase());
                    let other_fingerprint = r
                        .get("ssh_fingerprint")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_ascii_lowercase());
                    (mac.is_some() && mac == other_mac)
                        || (fingerprint.is_some() && fingerprint == other_fingerprint)
                }) {
                    let existing_score = router_score(&deduped[pos]);
                    let candidate_score = router_score(&router);
                    if candidate_score > existing_score {
                        let mut winner = router;
                        merge_missing(&mut winner, &deduped[pos]);
                        deduped[pos] = winner;
                    } else {
                        let mut winner = deduped[pos].clone();
                        merge_missing(&mut winner, &router);
                        deduped[pos] = winner;
                    }
                    continue;
                }
            }
            deduped.push(router);
        }

        Ok(serde_json::json!({
            "routers": deduped,
            "active": saved.address,
            "diagnostics": scan.get("diagnostics"),
            "failure": scan.get("failure"),
            "installer_version": cfg.get("installer_version"),
            "feed_release_tag": cfg.get("feed_release_tag"),
        }))
    }

    /// Make a discovered router the active one (and persist it).
    ///
    /// Verifies SSH/OpenWrt before saving, so a typo cannot replace a working
    /// saved router. `trust_fingerprint` optionally records the host key with
    /// the installer as well.
    pub(super) async fn handle_openwrt_select_router(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let p = params.unwrap_or_default();
        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("host is required"))?
            .to_string();
        let mac = p
            .get("mac")
            .and_then(|v| v.as_str())
            .map(|s| s.to_ascii_lowercase());
        let supplied_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let supplied_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let trust_fingerprint = p
            .get("trust_fingerprint")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let known = net_router::load_known_routers(&self.config.data_dir).await?;
        let active = net_router::load_router_config(&self.config.data_dir).await?;
        let remembered = net_router::find_known_router(&known, &host, mac.as_deref())
            .cloned()
            .or_else(|| {
                if active.configured
                    && active.address == host
                    && active
                        .password
                        .as_deref()
                        .map(|p| !p.is_empty())
                        .unwrap_or(false)
                {
                    Some(net_router::KnownRouter {
                        hostname: String::new(),
                        model: String::new(),
                        username: active
                            .username
                            .clone()
                            .unwrap_or_else(|| "root".to_string()),
                        password: active.password.clone().unwrap_or_default(),
                        last_seen: String::new(),
                    })
                } else {
                    None
                }
            });
        let used_saved = supplied_password.is_empty() && remembered.is_some();
        let ssh_user = supplied_user
            .or_else(|| remembered.as_ref().map(|r| r.username.clone()))
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = if !supplied_password.is_empty() {
            supplied_password
        } else {
            remembered
                .as_ref()
                .map(|r| r.password.clone())
                .unwrap_or_default()
        };
        let attempted_saved = remembered.is_some() || !ssh_password.is_empty();

        let host_check = host.clone();
        let user_check = ssh_user.clone();
        let pass_check = ssh_password.clone();
        let verified = tokio::task::spawn_blocking(move || -> Result<(String, String)> {
            let router = Router::connect_password(&host_check, 22, &user_check, &pass_check)?;
            router.verify_openwrt()?;
            let hostname = router
                .uci_get("system.@system[0].hostname")
                .unwrap_or_default();
            let model = router
                .run_ok("cat /tmp/sysinfo/model 2>/dev/null")
                .unwrap_or_default()
                .trim()
                .to_string();
            Ok((hostname, model))
        })
        .await
        .context("openwrt select-router verify task")?;

        let (hostname, model) = match verified {
            Ok(found) => found,
            Err(err) => {
                return Ok(serde_json::json!({
                    "ok": false,
                    "host": host,
                    "needs_password": true,
                    "bad_password": attempted_saved,
                    "error": err.to_string(),
                }));
            }
        };

        if let Some(fingerprint) = trust_fingerprint {
            let client = InstallerClient::new(&self.config.data_dir);
            client.ensure_running().await?;
            client.trust(&host, &fingerprint).await?;
        }

        let key = mac.clone().unwrap_or_else(|| host.clone());
        let model = if model.is_empty() {
            p.get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            model
        };
        net_router::remember_router(
            &self.config.data_dir,
            &key,
            &hostname,
            &model,
            &ssh_user,
            &ssh_password,
        )
        .await?;

        let saved = net_router::configure_router(
            &self.config.data_dir,
            net_router::RouterType::OpenWrt,
            &host,
            None,
            Some(&ssh_user),
            Some(&ssh_password),
        )
        .await?;
        Ok(serde_json::json!({
            "ok": true,
            "host": saved.address,
            "address": saved.address,
            "username": saved.username,
            "hostname": hostname,
            "model": model,
            "used_saved_credentials": used_saved,
        }))
    }

    /// Status/version of the local tollgate-installer service.
    pub(super) async fn handle_openwrt_installer_status(&self) -> Result<serde_json::Value> {
        let client = InstallerClient::new(&self.config.data_dir);
        match client.ensure_running().await {
            Ok(cfg) => Ok(serde_json::json!({
                "available": true,
                "installer_version": cfg.get("installer_version"),
                "installer_commit": cfg.get("installer_commit"),
                "feed_release_tag": cfg.get("feed_release_tag"),
                "feed_pkg_version": cfg.get("feed_pkg_version"),
                "feed_module_pin": cfg.get("feed_module_pin"),
            })),
            Err(err) => Ok(serde_json::json!({
                "available": false,
                "error": err.to_string(),
            })),
        }
    }

    /// Identify a router through the installer; returns `ssh_fingerprint` when
    /// the host key still needs an explicit trust decision.
    pub(super) async fn handle_openwrt_installer_identify(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();
        let ip = p
            .get("ip")
            .or_else(|| p.get("host"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| saved.configured.then(|| saved.address.clone()))
            .ok_or_else(|| anyhow::anyhow!("ip is required"))?;
        let password = p
            .get("password")
            .or_else(|| p.get("ssh_password"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| saved.password.clone())
            .unwrap_or_default();
        let client = InstallerClient::new(&self.config.data_dir);
        client.ensure_running().await?;
        client.identify(&ip, &password).await
    }

    /// Trust the router's SSH host key after the operator verified the
    /// fingerprint out of band.
    pub(super) async fn handle_openwrt_installer_trust(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();
        let ip = p
            .get("ip")
            .or_else(|| p.get("host"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| saved.configured.then(|| saved.address.clone()))
            .ok_or_else(|| anyhow::anyhow!("ip is required"))?;
        let fingerprint = p
            .get("fingerprint")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("fingerprint is required"))?;
        let client = InstallerClient::new(&self.config.data_dir);
        client.ensure_running().await?;
        client.trust(&ip, fingerprint).await
    }

    /// LAN discovery through the installer.
    pub(super) async fn handle_openwrt_installer_scan(&self) -> Result<serde_json::Value> {
        let client = InstallerClient::new(&self.config.data_dir);
        client.ensure_running().await?;
        client.scan().await
    }

    /// Installer build/feed identity without starting it.
    pub(super) async fn handle_openwrt_installer_config(&self) -> Result<serde_json::Value> {
        let client = InstallerClient::new(&self.config.data_dir);
        client.config().await
    }

    /// Provision TollGate on an OpenWrt router and create the "archipelago" SSID.
    ///
    /// Params: `{ "host": "192.168.1.1", "ssh_user": "root", "ssh_password": "",
    ///             "price_sats": 10, "step_size_ms": 60000, "min_steps": 1,
    ///             "mint_url": "<optional override>",
    ///             "payout_address": "<optional Lightning address>" }`
    ///
    /// `mint_url` defaults to `http://<this node's IP>:3338` — the local Cashu
    /// mint that must be running as an Archy app before calling this endpoint.
    ///
    /// `payout_address` sets the "owner" identity's Lightning address for
    /// TollGate's own built-in payout (see `config::apply_payout_identity`).
    /// Omitted or blank leaves whatever's already on the router untouched.
    pub(super) async fn handle_openwrt_provision_tollgate(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let default_mint_url = format!("http://{}:{}", self.config.host_ip, LOCAL_MINT_PORT);
        // Trim trailing slash(es): tollgate-wrt matches a token's embedded
        // mint URL against this value with an exact string compare, and
        // Cashu wallets (Minibits included) encode mint URLs without a
        // trailing slash. A stray slash here means every otherwise-valid
        // token gets rejected as "untrusted mint" — confirmed live against
        // archy-x250-pa3 2026-09-07 with a manually-entered
        // "https://mint.minibits.cash/Bitcoin/".
        let mint_url = p
            .get("mint_url")
            .and_then(|v| v.as_str())
            .unwrap_or(&default_mint_url)
            .trim_end_matches('/')
            .to_string();

        // `None` (not sent, or sent blank) leaves whatever's already on the
        // router untouched — see apply_payout_identity's doc comment for why
        // that matters (an upstream-default placeholder otherwise survives
        // forever, since nothing else ever writes this field).
        let payout_address = p
            .get("payout_address")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if let Some(address) = payout_address.as_deref() {
            tollgate::config::validate_payout_address(address)
                .context("invalid TollGate payout address")?;
        }

        // Keep the operator's price sane before touching the router. `0` is
        // not a free tier here: the daemon divides by the per-step price and
        // refuses to advertise a mint whose price is zero.
        let price_sats = p.get("price_sats").and_then(|v| v.as_u64()).unwrap_or(10);
        let step_size_ms = p
            .get("step_size_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(60_000);
        let min_steps = p.get("min_steps").and_then(|v| v.as_u64()).unwrap_or(1);
        if price_sats == 0 {
            anyhow::bail!("price_sats must be at least 1 sat");
        }
        if step_size_ms < 60_000 {
            anyhow::bail!("step_size_ms must be at least 60000 (one minute)");
        }
        if min_steps == 0 {
            anyhow::bail!("min_steps must be at least 1");
        }
        if min_steps > u32::MAX as u64 {
            anyhow::bail!("min_steps is too large");
        }

        let private_ssid = p
            .get("private_ssid")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("noderunner")
            .to_string();
        let private_key = p
            .get("private_key")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let accepted_mints: Option<Vec<tollgate::MintConfig>> = p
            .get("accepted_mints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect::<Vec<_>>()
            });
        if let Some(mints) = &accepted_mints {
            if mints.is_empty() {
                anyhow::bail!("accepted_mints must contain at least one mint");
            }
            for mint in mints {
                validate_mint_url(&mint.url)?;
                if mint.price_per_step == 0 {
                    anyhow::bail!("each mint's price_per_step must be at least 1 sat");
                }
                if mint.purchase_min_steps == 0 {
                    anyhow::bail!("each mint's purchase_min_steps must be at least 1");
                }
            }
        }
        let mint_url = accepted_mints
            .as_ref()
            .and_then(|mints| mints.first())
            .map(|mint| mint.url.clone())
            .unwrap_or(mint_url);

        let config = TollGateConfig {
            ssid: "archipelago".to_string(),
            mint_url,
            price_sats,
            step_size_ms,
            min_steps: min_steps as u32,
            enabled: p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            payout_address,
            private_ssid,
            private_key,
            accepted_mints,
        };

        let engine = p
            .get("engine")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("ARCHY_TOLLGATE_ENGINE").ok())
            .unwrap_or_else(|| "official".to_string());

        if engine == "native" {
            // Deprecated fallback retained for one release. The official
            // installer is the default; see
            // docs/architecture/one-provisioning-engine.md.
            let first = provision_tollgate_once(
                host.clone(),
                ssh_user.clone(),
                ssh_password.clone(),
                config.clone(),
            )
            .await;

            if let Err(err) = first {
                // The package's post-install restarts the network on a
                // radio-equipped router, which kills the SSH session running
                // `apk add` / `opkg install`. The package is already on disk when
                // that happens, so the dropped session is not evidence that the
                // install failed. Reconnect, wait for the router to come back, and
                // only then decide. Retrying the whole pass is safe: the install
                // decision skips an already-correct version, and the config/SSID
                // steps are exactly what the dropped session never reached.
                let desired = tollgate::package_version_from_pin(tollgate::TOLLGATE_VERSION)
                    .context("derive the pinned TollGate version")?;
                let mut at_pin = false;
                for attempt in 0..6 {
                    if attempt > 0 {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                    if tollgate_is_at_pin(
                        host.clone(),
                        ssh_user.clone(),
                        ssh_password.clone(),
                        desired.clone(),
                    )
                    .await
                    {
                        at_pin = true;
                        break;
                    }
                }
                if !at_pin {
                    return Err(err);
                }
                provision_tollgate_once(
                    host.clone(),
                    ssh_user.clone(),
                    ssh_password.clone(),
                    config.clone(),
                )
                .await?;
            }

            return Ok(serde_json::json!({
                "ok": true,
                "engine": "native",
                "host": host,
                "ssid": config.ssid,
                "mint_url": config.mint_url,
            }));
        }

        // Official installer: discovery/trust/flash/install/verify is its job.
        // Archy applies its own config/SSID/topology afterwards.
        let lnurl = config.payout_address.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "The official tollgate-installer requires a Lightning payout address \
                 (payout_address) — provide one in the install form"
            )
        })?;
        let installer = InstallerClient::new(&self.config.data_dir);
        let installer_cfg = installer
            .ensure_running()
            .await
            .context("start the local tollgate-installer")?;

        let identify = installer
            .identify(&host, &ssh_password)
            .await
            .context("identify the router through tollgate-installer")?;
        let presented = identify
            .get("ssh_fingerprint")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let trust_fp = p
            .get("trust_fingerprint")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if !presented.is_empty() && trust_fp != presented {
            return Ok(serde_json::json!({
                "ok": false,
                "engine": "official",
                "host": host,
                "host_key_untrusted": true,
                "ssh_fingerprint": presented,
                "message": "Verify this fingerprint on the router's console, then retry with trust_fingerprint set to it.",
            }));
        }
        if !trust_fp.is_empty() {
            installer
                .trust(&host, &trust_fp)
                .await
                .context("trust the router's SSH host key")?;
        }

        let mut deploy = serde_json::json!({
            "ip": host,
            "password": ssh_password,
            "lnurl": lnurl,
            "mode": p.get("mode").and_then(|v| v.as_str()).unwrap_or("wan"),
            "mint": config.mint_url.clone(),
            "test_mints": config
                .accepted_mints
                .as_ref()
                .map(|mints| mints.iter().any(|m| m.url.contains("testnut")))
                .unwrap_or(false),
            "forceFlash": p.get("force_flash").and_then(|v| v.as_bool()).unwrap_or(false),
        });
        for (src, dst) in [
            ("upstream_ssid", "ssid"),
            ("upstream_password", "wifiPass"),
            ("band", "band"),
            ("admin_password", "adminPassword"),
        ] {
            if let Some(v) = p
                .get(src)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                deploy[dst] = serde_json::json!(v);
            }
        }
        if let Some(key) = config.private_key.as_deref() {
            deploy["privateWifiPassword"] = serde_json::json!(key);
        }
        if let Some(v) = p
            .get("devSplit")
            .or_else(|| p.get("dev_split"))
            .and_then(|v| v.as_i64())
        {
            deploy["devSplit"] = serde_json::json!(v);
        }
        if let Some(v) = p.get("margin").and_then(|v| v.as_i64()) {
            deploy["margin"] = serde_json::json!(v);
        }

        let job = installer
            .deploy(deploy)
            .await
            .context("start the tollgate-installer deploy")?;
        let snapshot = installer
            .wait(&job, Duration::from_secs(900))
            .await
            .with_context(|| format!("tollgate-installer job {job}"))?;

        // The package's post-install restart can move the wired management
        // address onto br-private. The installer records the address it ended
        // on in the job snapshot.
        let final_host = snapshot
            .get("ip")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&host)
            .to_string();
        // The installer may have generated a root credential; it is served
        // once on the terminal status read. Use it for the post-config SSH.
        let post_password = snapshot
            .get("generated_password")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                p.get("admin_password")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| ssh_password.clone());

        provision_after_install_once(
            final_host.clone(),
            ssh_user.clone(),
            post_password,
            config.clone(),
        )
        .await
        .context("apply the Archy TollGate configuration after the installer")?;

        Ok(serde_json::json!({
            "ok": true,
            "engine": "official",
            "host": final_host,
            "ssid": config.ssid,
            "mint_url": config.mint_url,
            "job_id": job,
            "installer_version": installer_cfg.get("installer_version"),
            "feed_release_tag": installer_cfg.get("feed_release_tag"),
            "feed_pkg_version": installer_cfg.get("feed_pkg_version"),
        }))
    }

    /// Scan for visible WiFi networks from the router's radio.
    ///
    /// Params: same host/credentials as other openwrt methods.
    pub(super) async fn handle_openwrt_scan_wifi(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        // A radio scan is seconds of SSH round-trips even on a healthy
        // router; keep it off the runtime.
        let networks = {
            let host = host.clone();
            let ssh_user = ssh_user.clone();
            let ssh_password = ssh_password.clone();
            tokio::task::spawn_blocking(move || -> Result<Vec<wifi_scan::ScannedNetwork>> {
                let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
                router.verify_openwrt()?;
                wifi_scan::scan_networks(&router)
            })
            .await
            .context("openwrt wifi scan task")??
        };
        let result: Vec<serde_json::Value> = networks
            .iter()
            .map(|n| {
                serde_json::json!({
                    "ssid":       n.ssid,
                    "bssid":      n.bssid,
                    "signal":     n.signal,
                    "channel":    n.channel,
                    "encryption": n.encryption,
                })
            })
            .collect();

        Ok(serde_json::json!({ "networks": result }))
    }

    /// Configure WAN/WISP — connect the router to an upstream WiFi network.
    ///
    /// Params: host/credentials + `{ "ssid": "...", "password": "...", "encryption": "psk2" }`
    pub(super) async fn handle_openwrt_configure_wan(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let mut ssid = p
            .get("ssid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut password = p
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // "Use this node's Wi-Fi credentials": resolve the PSK here, in the
        // backend. The browser only ever sees the SSID and `has_password`.
        if p.get("use_node_credentials")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            if let Ok(Some((node_ssid, node_psk))) = super::interfaces::upstream_wifi_psk().await {
                if ssid.trim().is_empty() {
                    ssid = node_ssid.clone();
                }
                if password.is_empty() && ssid == node_ssid {
                    password = node_psk;
                }
            }
        }
        if ssid.trim().is_empty() {
            anyhow::bail!("Missing required field: ssid");
        }
        let encryption = p
            .get("encryption")
            .and_then(|v| v.as_str())
            .unwrap_or("psk2")
            .to_string();
        let dhcp_start = p.get("dhcp_start").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
        let dhcp_limit = p.get("dhcp_limit").and_then(|v| v.as_u64()).unwrap_or(150) as u32;
        let masq = p.get("masq").and_then(|v| v.as_bool()).unwrap_or(true);

        let config = wan::WispConfig {
            ssid: ssid.clone(),
            password,
            encryption,
            dhcp_start,
            dhcp_limit,
            masq,
        };
        // Reconfiguring WAN drops and re-establishes the router's uplink, so
        // the SSH exchange can stall for its full timeout budget mid-command.
        {
            let host = host.clone();
            let ssh_user = ssh_user.clone();
            let ssh_password = ssh_password.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
                router.verify_openwrt()?;
                wan::configure_wisp(&router, &config)
            })
            .await
            .context("openwrt configure-wan task")??;
        }

        Ok(serde_json::json!({ "ok": true, "host": host, "ssid": ssid }))
    }
}

/// Parse /etc/openwrt_release key=value pairs into a JSON object.
fn parse_release(raw: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            m.insert(
                k.to_lowercase(),
                serde_json::Value::String(v.trim_matches('"').to_string()),
            );
        }
    }
    serde_json::Value::Object(m)
}

/// Extract AP wifi-iface sections from `uci show wireless` output.
fn parse_wifi_interfaces(raw: &str) -> Vec<serde_json::Value> {
    use std::collections::HashMap;
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();

    for line in raw.lines() {
        if let Some((lhs, rhs)) = line.trim().split_once('=') {
            let parts: Vec<&str> = lhs.splitn(3, '.').collect();
            if parts.len() == 3 && parts[0] == "wireless" {
                sections
                    .entry(parts[1].to_string())
                    .or_default()
                    .insert(parts[2].to_string(), rhs.trim_matches('\'').to_string());
            }
        }
    }

    let mut ifaces: Vec<serde_json::Value> = sections
        .into_iter()
        .filter(|(_, f)| f.get("mode").map(|m| m == "ap").unwrap_or(false))
        .map(|(name, f)| {
            serde_json::json!({
                "section":    name,
                "ssid":       f.get("ssid").cloned().unwrap_or_default(),
                "device":     f.get("device").cloned().unwrap_or_default(),
                "encryption": f.get("encryption").cloned().unwrap_or_else(|| "none".into()),
                "network":    f.get("network").cloned().unwrap_or_default(),
                "disabled":   f.get("disabled").map(|v| v == "1").unwrap_or(false),
            })
        })
        .collect();

    ifaces.sort_by_key(|v| v["section"].as_str().unwrap_or("").to_string());
    ifaces
}

fn parse_ipv4(s: &str) -> Result<[u8; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        anyhow::bail!("Invalid IPv4: {}", s);
    }
    Ok([
        parts[0].parse()?,
        parts[1].parse()?,
        parts[2].parse()?,
        parts[3].parse()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_live_kind_10021_advertisement() {
        let event = r#"{
            "kind": 10021,
            "tags": [
                ["metric", "milliseconds"],
                ["step_size", "60000"],
                ["price_per_step", "cashu", "10", "sat", "http://192.168.50.199:3338", "1"],
                ["price_per_step", "cashu", "21", "sat", "https://mint.example/Bitcoin", "2"]
            ]
        }"#;
        let mints = parse_advertisement(event);
        assert_eq!(mints.len(), 2);
        assert_eq!(mints[0].url, "http://192.168.50.199:3338");
        assert_eq!(mints[0].price, 10);
        assert_eq!(mints[1].price, 21);
        assert_eq!(mints[1].min_steps, 2);
    }

    #[test]
    fn ignores_malformed_price_tags_instead_of_panicking() {
        let event = r#"{"kind":10021,"tags":[
            ["price_per_step"],
            ["price_per_step","cashu","not-a-number","sat","https://x","1"],
            ["price_per_step","cashu","1","sat",""],
            ["price_per_step","cashu","1","sat","https://ok","1"]
        ]}"#;
        let mints = parse_advertisement(event);
        assert_eq!(mints.len(), 1);
        assert_eq!(mints[0].url, "https://ok");
    }

    #[test]
    fn parses_a_healthy_daemon_status() {
        let out = r#"{"success":true,"data":{"running":true,
            "version":"TollGate v0.6.0-g9364f2e","uptime":"9.8s",
            "config_ok":true,"wallet_ok":true,"network_ok":true}}"#;
        let status = parse_daemon_status(out).expect("status");
        assert!(status.running);
        assert_eq!(status.version, "TollGate v0.6.0-g9364f2e");
        assert_eq!(status.uptime, "9.8s");
        assert!(status.error.is_none());
    }

    #[test]
    fn parses_a_stale_daemon_status_as_an_error_not_a_crash() {
        let out = r#"{"success":false,"error":"dial unix /var/run/tollgate.sock: connect: connection refused"}"#;
        let status = parse_daemon_status(out).expect("status");
        assert!(!status.running);
        assert!(status.error.unwrap().contains("connection refused"));
    }

    #[test]
    fn daemon_version_matches_the_package_after_normalisation() {
        assert!(daemon_version_matches("TollGate v0.6.0-g9364f2e", "0.6.0"));
        assert!(daemon_version_matches("v0.6.0", "0.6.0"));
        assert!(daemon_version_matches("TollGate v0.6.0", "0.6.0-r1"));
        assert!(!daemon_version_matches("TollGate v0.5.0-gabc", "0.6.0"));
        assert!(!daemon_version_matches("", "0.6.0"));
    }
}
