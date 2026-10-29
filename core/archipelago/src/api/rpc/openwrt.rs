use super::RpcHandler;
use crate::network::router as net_router;
use anyhow::{Context, Result};
use archipelago_openwrt::{
    detect,
    opkg::PkgManager,
    router::Router,
    tollgate::{self, version::Installed, TollGateConfig},
    wan, wifi_scan,
};
use serde::Deserialize;

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
        tokio::runtime::Handle::current().block_on(tollgate::provision(&router, &config))
    })
    .await
    .context("openwrt provision task")?
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
                    let desired = tollgate::package_version_from_pin(tollgate::TOLLGATE_VERSION)
                        .unwrap_or_else(|_| tollgate::TOLLGATE_VERSION.trim_start_matches('v').to_string());
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
                    let advertised_mints = router
                        .run_ok("curl -sS -m 5 http://127.0.0.1:2121/ 2>/dev/null")
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
            let base = url.trim_end_matches('/');
            let info_out =
                router.run_ok(&format!("curl -sS -m 10 '{base}/v1/info' 2>/dev/null"))?;
            let info: serde_json::Value =
                serde_json::from_str(&info_out).unwrap_or_else(|_| serde_json::json!({}));
            let keys_out =
                router.run_ok(&format!("curl -sS -m 10 '{base}/v1/keysets' 2>/dev/null"))?;
            let keys: serde_json::Value =
                serde_json::from_str(&keys_out).unwrap_or_else(|_| serde_json::json!({}));
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
        })
        .await
        .context("openwrt mint test task")??;

        Ok(result)
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
            .unwrap_or("noderunner-private")
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

        Ok(serde_json::json!({
            "ok": true,
            "host": host,
            "ssid": config.ssid,
            "mint_url": config.mint_url,
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

        let ssid = p
            .get("ssid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required field: ssid"))?
            .to_string();
        let password = p
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
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
