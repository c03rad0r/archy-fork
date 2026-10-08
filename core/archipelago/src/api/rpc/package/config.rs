use super::validation::validate_app_id;
use crate::port_allocator::PortAllocator;
use anyhow::{Context, Result};
use std::time::Duration;

const PODMAN_LIST_TIMEOUT: Duration = Duration::from_secs(60);

fn is_platform_managed_app(app_id: &str) -> bool {
    matches!(
        app_id,
        "bitcoin"
            | "bitcoin-core"
            | "bitcoin-knots"
            | "bitcoin-ui"
            | "lnd"
            | "lnd-ui"
            | "electrumx"
            | "electrs"
            | "mempool-electrs"
            | "electrs-ui"
            | "mempool"
            | "mempool-web"
            | "mempool-api"
            | "archy-mempool-db"
            | "archy-mempool-web"
            | "btcpay"
            | "btcpay-server"
            | "btcpayserver"
            | "archy-btcpay-db"
            | "archy-nbxplorer"
            | "fedimint"
            | "fedimint-gateway"
            | "indeedhub"
            | "immich"
            | "fips"
            | "fips-ui"
    )
}

fn safe_dynamic_arg(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.chars().any(|c| matches!(c, '\0' | '\n' | '\r'))
}

async fn dynamic_app_config(
    app_id: &str,
) -> Option<(
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Option<String>,
    Option<Vec<String>>,
)> {
    if is_platform_managed_app(app_id) {
        return None;
    }
    let config_path = format!("/var/lib/archipelago/app-configs/{}.json", app_id);
    let data = tokio::fs::read_to_string(&config_path).await.ok()?;
    let cfg = serde_json::from_str::<serde_json::Value>(&data).ok()?;
    let string_array = |key: &str| -> Vec<String> {
        cfg.get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| safe_dynamic_arg(s))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };

    let command = cfg
        .get("command")
        .and_then(|v| v.as_str())
        .filter(|s| safe_dynamic_arg(s))
        .map(String::from);
    let args = cfg.get("args").and_then(|v| v.as_array()).map(|a| {
        a.iter()
            .filter_map(|v| v.as_str())
            .filter(|s| safe_dynamic_arg(s))
            .map(String::from)
            .collect::<Vec<_>>()
    });

    tracing::info!(app_id = %app_id, "loaded catalog runtime config for generic app");
    Some((
        string_array("ports"),
        string_array("volumes"),
        string_array("env"),
        command,
        args.filter(|a| !a.is_empty()),
    ))
}

/// Validate a Docker image reference. Delegates to the shared policy in
/// `container::image_policy` — the same rules the orchestrator enforces at
/// its pull sites, so the two layers can't drift apart.
pub(super) fn is_valid_docker_image(image: &str) -> bool {
    crate::container::image_policy::is_valid_docker_image(image)
}

/// Per-app Linux capabilities needed beyond the default cap-drop=ALL.
/// Most apps need CHOWN/SETUID/SETGID for internal user switching.
pub(super) fn get_app_capabilities(app_id: &str) -> Vec<String> {
    match app_id {
        // Apps that need user switching and file ownership changes
        // Home Assistant needs NET_RAW for DHCP discovery
        "homeassistant" | "home-assistant" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=FOWNER".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=DAC_OVERRIDE".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
            "--cap-add=NET_RAW".to_string(),
        ],
        "nextcloud" | "btcpay-server" | "btcpayserver" | "portainer" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=DAC_OVERRIDE".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
        ],
        // Jellyfin: CoreCLR needs exec-enabled tmpfs for JIT compilation
        "jellyfin" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=FOWNER".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=DAC_OVERRIDE".to_string(),
        ],
        // Nginx Proxy Manager initializes/chowns mounted state on first boot.
        "nginx-proxy-manager" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=FOWNER".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=DAC_OVERRIDE".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
        ],
        // Bitcoin needs only file-ownership ops + NET_BIND_SERVICE for the
        // RPC port. NO NET_RAW — bitcoind never opens raw sockets and
        // dropping it removes a class of intra-pod spoofing capability.
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=FOWNER".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=DAC_OVERRIDE".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
        ],
        // LND additionally needs NET_RAW for TLS certificate generation
        // (netlink interface enumeration during `lnd --tlscertpath` first run).
        // Fedimint inherits the same set because the gateway also enumerates
        // network interfaces on startup.
        "lnd" | "fedimint" | "fedimint-gateway" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=FOWNER".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=DAC_OVERRIDE".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
            "--cap-add=NET_RAW".to_string(),
        ],
        // Vaultwarden needs file ownership + NET_BIND_SERVICE (binds port 80 internally)
        "vaultwarden" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
        ],
        // PhotoPrism uses s6-overlay which needs privilege ops
        "photoprism" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
        ],
        // Grafana runs as specific UID (472)
        "grafana" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
        ],
        // Uptime-kuma startup script needs chown/fowner for /app/data ownership
        "uptime-kuma" => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=FOWNER".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
        ],
        // FileBrowser needs DAC_OVERRIDE for volume access + NET_BIND_SERVICE to bind port 80
        "filebrowser" => vec![
            "--cap-add=DAC_OVERRIDE".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
        ],
        // VPN/mesh daemons need TUN + NET_ADMIN.
        // Note: --device=/dev/net/tun is added separately in install.rs
        "nostr-vpn" | "fips" => vec![
            "--cap-add=NET_ADMIN".to_string(),
            "--cap-add=NET_RAW".to_string(),
        ],
        // Default: standard capabilities for rootless podman containers
        // Most apps need file ownership + port binding to function correctly
        _ => vec![
            "--cap-add=CHOWN".to_string(),
            "--cap-add=FOWNER".to_string(),
            "--cap-add=SETUID".to_string(),
            "--cap-add=SETGID".to_string(),
            "--cap-add=DAC_OVERRIDE".to_string(),
            "--cap-add=NET_BIND_SERVICE".to_string(),
        ],
    }
}

/// Apps safe to run with --read-only root filesystem.
/// These work correctly with volume mounts + tmpfs for /tmp and /run.
pub(super) fn is_readonly_compatible(app_id: &str) -> bool {
    matches!(
        app_id,
        "searxng"
            | "grafana"
            | "filebrowser"
            | "electrumx"
            | "mempool-electrs"
            | "electrs"
            | "nostr-rs-relay"
            | "ollama"
            | "indeedhub"
    )
}

/// Get container health check arguments for podman run.
/// Returns (health-cmd, interval, retries) args to append to run_args.
pub(super) fn get_health_check_args(app_id: &str, _rpc_pass: &str) -> Vec<String> {
    let (cmd, interval, retries) = match app_id {
        // Bitcoin images do not consistently ship bitcoin-cli/curl/nc. Rely on
        // process state here; manifests still describe the desired TCP check.
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => return vec![],
        "lnd" => ("lncli getinfo || exit 1", "30s", "3"),
        "btcpay-server" | "btcpayserver" => {
            ("bash -ec '</dev/tcp/127.0.0.1/49392'", "30s", "3")
        }
        "mempool-api" => (
            http_probe_cmd("http://localhost:8999/api/v1/backend-info"),
            "30s",
            "3",
        ),
        "mempool" | "mempool-web" | "archy-mempool-web" => {
            (http_probe_cmd("http://localhost:8080/"), "30s", "3")
        }
        "electrumx" | "mempool-electrs" | "electrs" => {
            ("curl -sf http://localhost:8000/ || exit 1", "60s", "3")
        }
        "nextcloud" => (
            "curl -s -o /dev/null http://localhost:80/status.php || exit 1",
            "30s",
            "3",
        ),
        "homeassistant" | "home-assistant" => {
            ("curl -sf http://localhost:8123/ || exit 1", "30s", "3")
        }
        "grafana" => (
            "test -w /var/lib/grafana && test -w /var/lib/grafana/grafana.db && curl -sf http://localhost:3000/api/health || exit 1",
            "30s",
            "3",
        ),
        "jellyfin" => (
            "curl -sf http://localhost:8096/health || exit 1",
            "30s",
            "3",
        ),
        "vaultwarden" => ("curl -sf http://localhost:80/alive || exit 1", "30s", "3"),
        "uptime-kuma" => ("curl -sf http://localhost:3001/ || exit 1", "30s", "3"),
        "filebrowser" => ("curl -sf http://localhost:80/health || exit 1", "30s", "3"),
        "botfights" => (
            "node -e \"fetch(\\\"http://127.0.0.1:9100/api/health\\\").then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))\"",
            "30s",
            "3",
        ),
        "searxng" => (http_probe_cmd("http://localhost:8080/"), "30s", "3"),
        "photoprism" => (
            "curl -sf http://localhost:2342/api/v1/status || exit 1",
            "60s",
            "3",
        ),
        "immich_server" | "immich" => (
            "curl -sf http://localhost:2283/api/server/ping || exit 1",
            "30s",
            "3",
        ),
        "portainer" => return vec![],
        "ollama" => ("curl -sf http://localhost:11434/ || exit 1", "30s", "3"),
        "fedimint" => ("curl -sf http://localhost:8175/ || exit 1", "60s", "3"),
        "fedimint-gateway" => ("curl -sf http://localhost:8176/ || exit 1", "60s", "3"),
        "nostr-rs-relay" | "nostr-relay" => (http_probe_cmd("http://localhost:8080/"), "30s", "3"),
        "nginx-proxy-manager" => (http_probe_cmd("http://localhost:81/"), "30s", "3"),
        "routstr" => (
            "curl -sf http://localhost:8000/v1/models || exit 1",
            "30s",
            "3",
        ),
        "nostr-vpn" => ("nvpn status || exit 1", "30s", "3"),
        "fips" => ("fipsctl status || exit 1", "30s", "3"),
        _ => return vec![],
    };

    vec![
        format!("--health-cmd={}", cmd),
        format!("--health-interval={}", interval),
        format!("--health-retries={}", retries),
        "--health-timeout=10s".to_string(),
        "--health-start-period=60s".to_string(),
    ]
}

fn http_probe_cmd(url: &'static str) -> &'static str {
    match url {
        "http://localhost:8999/api/v1/backend-info" => "if command -v wget >/dev/null 2>&1; then wget -q -T 5 -O /dev/null http://localhost:8999/api/v1/backend-info; elif command -v curl >/dev/null 2>&1; then curl -fsS -m 5 http://localhost:8999/api/v1/backend-info; else exit 0; fi",
        "http://localhost:8080/" => "if command -v wget >/dev/null 2>&1; then wget -q -T 5 -O /dev/null http://localhost:8080/; elif command -v curl >/dev/null 2>&1; then curl -fsS -m 5 http://localhost:8080/; else exit 0; fi",
        "http://localhost:81/api/" => "if command -v wget >/dev/null 2>&1; then wget -q -T 5 -O /dev/null http://localhost:81/api/; elif command -v curl >/dev/null 2>&1; then curl -fsS -m 5 http://localhost:81/api/; else exit 0; fi",
        "http://localhost:81/" => "if command -v wget >/dev/null 2>&1; then wget -q -T 5 -O /dev/null http://localhost:81/; elif command -v curl >/dev/null 2>&1; then curl -fsS -m 5 http://localhost:81/; else exit 0; fi",
        _ => "exit 0",
    }
}

/// Bitcoin UTXO cache (`-dbcache`) in MB, sized to host RAM.
///
/// A fixed large dbcache on a small box pushes bitcoind + the ~20 app
/// containers past physical RAM and triggers system-wide swap thrash: the
/// disk saturates, bitcoind can't answer its own RPC, and the dashboard
/// backend's sqlite reads stall — surfacing as /rpc/v1 502s and a blank
/// Bitcoin UI. Budget ~1/16 of RAM for the cache (floor 300 MB — bitcoind's
/// own default is 450 — cap 4096 MB), mirroring scripts/container-specs.sh.
pub(super) fn bitcoin_dbcache_mb() -> u64 {
    let total_mb = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|c| {
            c.lines()
                .find_map(|l| l.strip_prefix("MemTotal:"))
                .and_then(|v| v.split_whitespace().next())
                .and_then(|kb| kb.parse::<u64>().ok())
        })
        .map(|kb| kb / 1024)
        .unwrap_or(16000); // assume a comfortable host if /proc/meminfo is unreadable
    (total_mb / 16).clamp(300, 4096)
}

/// Get per-app memory limit.
pub(super) fn get_memory_limit(app_id: &str) -> &'static str {
    match app_id {
        // Heavy apps. Bitcoin: dbcache is now host-RAM-aware (see
        // bitcoin_dbcache_mb), so the daemon's footprint scales with the box.
        // This cgroup cap is an upper bound for mempool + connection buffers +
        // script-verifier memory + I/O; a tight cap (4g) previously caused
        // OOM-cascades during IBD, so keep 8g as a generous ceiling rather
        // than a tight limit — swap thrash is prevented at the dbcache layer.
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => "8g",
        // ElectrumX indexing spikes above its cache size due Python,
        // RocksDB, socket buffers, and reorg/history work. Keep cache
        // conservative and give the process headroom to avoid restart loops.
        "electrumx" | "mempool-electrs" | "electrs" => "6g",
        "cryptpad" => "512m",
        "ollama" => "4g",
        // Medium apps
        "lnd" => "512m",
        "nextcloud" => "1g",
        "immich_server" | "immich" => "1g",
        "btcpay-server" | "btcpayserver" => "1g",
        "homeassistant" | "home-assistant" => "512m",
        "fedimint" => "512m",
        "fedimint-gateway" => "512m",
        "photoprism" => "1g",
        // Light apps
        "mempool-api" => "512m",
        "mempool" | "mempool-web" | "archy-mempool-web" => "256m",
        "grafana" => "256m",
        "jellyfin" => "1g",
        "vaultwarden" => "256m",
        "uptime-kuma" => "256m",
        "filebrowser" => "256m",
        "searxng" => "512m",
        "portainer" => "256m",
        "nostr-rs-relay" | "nostr-relay" => "256m",
        "routstr" => "512m",
        "nostr-vpn" => "256m",
        "netbird" => "1g",
        "fips" => "256m",
        "nginx-proxy-manager" => "256m",
        // Databases
        "archy-btcpay-db" | "archy-mempool-db" | "mysql-mempool" => "512m",
        "immich_postgres" => "2g",
        "penpot-postgres" => "256m",
        "immich_redis" | "penpot-valkey" => "128m",
        // Default
        _ => "512m",
    }
}

/// Get all container names for an app (handles multi-container apps like mempool)
/// All known container name variants for a given app ID.
/// This is the single source of truth for container name resolution.
/// Every name that could appear in `podman ps` for this app must be listed here.
pub(super) fn all_container_names(package_id: &str) -> Vec<String> {
    let base = package_id.to_string();
    let archy = format!("archy-{}", package_id);

    match package_id {
        // Bitcoin variants share the UI but not the backend process. Keep
        // backend names precise so stopping one implementation does not clear
        // stop markers or issue podman operations for the other.
        "bitcoin" | "bitcoin-knots" => vec![
            "bitcoin-knots".into(),
            "bitcoin".into(),
            "archy-bitcoin-knots".into(),
            "archy-bitcoin".into(),
            "bitcoin-ui".into(),
            "archy-bitcoin-ui".into(),
        ],
        "bitcoin-core" => vec![
            "bitcoin-core".into(),
            "archy-bitcoin-core".into(),
            "bitcoin-ui".into(),
            "archy-bitcoin-ui".into(),
        ],
        // LND + UI
        "lnd" => vec!["lnd".into(), "archy-lnd".into(), "archy-lnd-ui".into()],
        // Electrumx: multiple aliases
        "electrumx" | "electrs" | "mempool-electrs" => vec![
            "electrumx".into(),
            "electrs".into(),
            "mempool-electrs".into(),
            "archy-electrumx".into(),
            "archy-electrs-ui".into(),
        ],
        // Mempool: multi-container stack
        "mempool" | "mempool-web" => vec![
            "mempool".into(),
            "mempool-web".into(),
            "mempool-api".into(),
            "archy-mempool-web".into(),
            "archy-mempool-api".into(),
            "archy-mempool-db".into(),
            "mysql-mempool".into(),
        ],
        // BTCPay: multi-container + multiple aliases
        "btcpay-server" | "btcpayserver" | "btcpay" => vec![
            "btcpay-server".into(),
            "btcpay".into(),
            "btcpayserver".into(),
            "archy-btcpay".into(),
            "archy-btcpay-db".into(),
            "archy-nbxplorer".into(),
        ],
        // Home Assistant: two naming conventions
        "homeassistant" | "home-assistant" => vec![
            "homeassistant".into(),
            "home-assistant".into(),
            "archy-homeassistant".into(),
        ],
        // Fedimint: multiple related containers
        "fedimint" => vec![
            "fedimint".into(),
            "fedimintd".into(),
            "fedimint-ui".into(),
            "archy-fedimint".into(),
            "fedimint-gateway".into(),
        ],
        "fedimint-gateway" => vec!["fedimint-gateway".into()],
        // Immich: multi-container
        "immich" => vec![
            "immich_postgres".into(),
            "immich_redis".into(),
            "immich_server".into(),
        ],
        // Penpot: multi-container
        "penpot" | "penpot-frontend" => vec![
            "penpot-postgres".into(),
            "penpot-valkey".into(),
            "penpot-backend".into(),
            "penpot-exporter".into(),
            "penpot-frontend".into(),
        ],
        "indeedhub" => vec![
            "indeedhub-postgres".into(),
            "indeedhub-redis".into(),
            "indeedhub-minio".into(),
            "indeedhub-relay".into(),
            "indeedhub-api".into(),
            "indeedhub-ffmpeg".into(),
            "indeedhub".into(),
        ],
        "netbird" => vec![
            "netbird".into(),
            "netbird-dashboard".into(),
            "netbird-server".into(),
        ],
        // Pine voice-assistant stack: launcher + the two Wyoming engines.
        "pine" => vec![
            "pine".into(),
            "pine-whisper".into(),
            "pine-piper".into(),
            "pine-openwakeword".into(),
        ],
        "nostr-vpn" => vec![
            "nostr-vpn".into(),
            "archy-nostr-vpn".into(),
            "archy-nostr-vpn-ui".into(),
        ],
        "fips" => vec!["fips".into(), "archy-fips".into(), "archy-fips-ui".into()],
        "routstr" => vec!["routstr".into(), "archy-routstr".into()],
        // Default: exact name + archy- prefix
        _ => vec![base, archy],
    }
}

/// Find all running/stopped containers that belong to a given app.
/// Uses the canonical name list from all_container_names().
pub(in crate::api::rpc) async fn get_containers_for_app(package_id: &str) -> Result<Vec<String>> {
    validate_app_id(package_id)?;
    let mut cmd = tokio::process::Command::new("podman");
    cmd.args(["ps", "-a", "--format", "{{.Names}}"]);
    cmd.kill_on_drop(true);
    let output = tokio::time::timeout(PODMAN_LIST_TIMEOUT, cmd.output())
        .await
        .context("podman ps timed out while listing containers")?
        .context("Failed to list containers")?;
    containers_from_list_output(package_id, &output)
}

fn containers_from_list_output(
    package_id: &str,
    output: &std::process::Output,
) -> Result<Vec<String>> {
    anyhow::ensure!(
        output.status.success(),
        "podman ps failed while listing containers"
    );
    let stdout = std::str::from_utf8(&output.stdout).context("Invalid container list response")?;
    let all: Vec<&str> = stdout.lines().filter(|s| !s.is_empty()).collect();

    let patterns = all_container_names(package_id);
    let mut result = Vec::new();
    for name in all {
        if patterns.iter().any(|p| p == name) {
            result.push(name.to_string());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{all_container_names, get_data_dirs_for_app, get_health_check_args};

    #[test]
    fn failed_container_listing_is_not_an_absent_app() {
        use std::os::unix::process::ExitStatusExt;
        let mut output = std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: vec![],
            stderr: b"store unavailable".to_vec(),
        };
        assert!(super::containers_from_list_output("node-demo-music", &output).is_err());
        output.stdout = b"node-demo-music\n".to_vec();
        assert!(super::containers_from_list_output("node-demo-music", &output).is_err());
        output.status = std::process::ExitStatus::from_raw(0);
        assert_eq!(
            super::containers_from_list_output("node-demo-music", &output).unwrap(),
            vec!["node-demo-music"]
        );
        output.stdout.clear();
        assert!(
            super::containers_from_list_output("node-demo-music", &output)
                .unwrap()
                .is_empty()
        );
        output.stdout = vec![0xff];
        assert!(super::containers_from_list_output("node-demo-music", &output).is_err());
    }

    #[test]
    fn bitcoin_variant_container_names_are_precise() {
        let core = all_container_names("bitcoin-core");
        assert!(core.contains(&"bitcoin-core".to_string()));
        assert!(!core.contains(&"bitcoin-knots".to_string()));

        let knots = all_container_names("bitcoin-knots");
        assert!(knots.contains(&"bitcoin-knots".to_string()));
        assert!(!knots.contains(&"bitcoin-core".to_string()));
    }

    #[test]
    fn grafana_health_requires_writable_data_and_http_health() {
        let args = get_health_check_args("grafana", "unused");
        let health_cmd = args
            .iter()
            .find_map(|arg| arg.strip_prefix("--health-cmd="))
            .expect("grafana should have a health command");

        assert!(health_cmd.contains("test -w /var/lib/grafana"));
        assert!(health_cmd.contains("test -w /var/lib/grafana/grafana.db"));
        assert!(health_cmd.contains("http://localhost:3000/api/health"));
    }

    /// The BTCPay wipe bug (operator report 2026-08-07): a wipe must remove
    /// the postgres volume — that is where the account lives — plus nbxplorer
    /// and the app's own dir. The pre-fix map removed only `…/btcpay`, so a
    /// "fresh" reinstall came back with the old account still enabled.
    #[test]
    fn btcpay_wipe_includes_postgres_and_nbxplorer() {
        let dirs = get_data_dirs_for_app("btcpay-server");
        assert!(dirs.contains(&"/var/lib/archipelago/btcpay".to_string()));
        assert!(dirs.contains(&"/var/lib/archipelago/postgres-btcpay".to_string()));
        assert!(dirs.contains(&"/var/lib/archipelago/nbxplorer".to_string()));
        // Aliases and the stack-member ids must map to the same set.
        assert_eq!(get_data_dirs_for_app("btcpay"), dirs);
        assert_eq!(get_data_dirs_for_app("archy-btcpay-db"), dirs);
    }
}

/// Get data directories to clean for an app.
/// Caller must validate package_id before calling.
pub(super) fn get_data_dirs_for_app(package_id: &str) -> Vec<String> {
    let base = "/var/lib/archipelago";
    match package_id {
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => {
            vec![format!("{}/bitcoin", base), format!("{}/bitcoin-ui", base)]
        }
        "mempool" | "mempool-web" => vec![
            format!("{}/mempool", base),
            format!("{}/mysql-mempool", base),
            format!("{}/electrumx", base),
        ],
        // The whole btcpay stack — the default arm's lone `…/btcpay` left
        // postgres-btcpay (accounts!) and nbxplorer behind, so a
        // wipe-and-reinstall came back with the old account still enabled
        // (operator report, 2026-08-07). This list is deliberately hardcoded
        // and reviewed: deletion code must never derive its targets from a
        // manifest at uninstall time (a bad manifest could aim the wipe).
        "btcpay-server" | "btcpayserver" | "btcpay" | "archy-btcpay-db" | "archy-nbxplorer" => {
            vec![
                format!("{}/btcpay", base),
                format!("{}/postgres-btcpay", base),
                format!("{}/nbxplorer", base),
            ]
        }
        "fedimint" => vec![
            format!("{}/fedimint", base),
            format!("{}/fedimint-gateway", base),
        ],
        "fedimint-gateway" => vec![format!("{}/fedimint-gateway", base)],
        "immich" => vec![format!("{}/immich", base), format!("{}/immich-db", base)],
        "penpot" | "penpot-frontend" => vec![
            format!("{}/penpot-assets", base),
            format!("{}/penpot-postgres", base),
        ],
        "netbird" => vec![format!("{}/netbird", base)],
        _ => vec![format!("{}/{}", base, package_id)],
    }
}

/// Read a secret from /var/lib/archipelago/secrets/{name}.
/// Falls back to the provided default if the file doesn't exist.
fn read_secret(name: &str, default: &str) -> String {
    let path = format!("/var/lib/archipelago/secrets/{}", name);
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| default.to_string())
}

/// Read a secret or generate and persist a random one if it doesn't exist.
pub(super) async fn read_or_generate_secret(name: &str) -> String {
    let path = format!("/var/lib/archipelago/secrets/{}", name);
    if let Ok(val) = tokio::fs::read_to_string(&path).await {
        let trimmed = val.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    // Generate a 24-byte random password (hex-encoded = 48 chars)
    let mut buf = [0u8; 24];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut buf);
    let secret = hex::encode(buf);
    let _ = tokio::fs::create_dir_all("/var/lib/archipelago/secrets").await;
    let _ = tokio::fs::write(&path, &secret).await;
    secret
}

/// Read the node-level Nostr secret key (hex) for identity-aware apps.
/// Returns empty string if not yet generated.
fn read_nostr_secret_hex() -> String {
    std::fs::read_to_string("/var/lib/archipelago/identity/nostr_secret")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Read the node-level Nostr public key (hex).
fn read_nostr_pubkey_hex() -> String {
    std::fs::read_to_string("/var/lib/archipelago/identity/nostr_pub")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Get app-specific configuration
/// Returns: (ports, volumes, env_vars, custom_command, custom_args)
pub(super) async fn get_app_config(
    app_id: &str,
    host_ip: &str,
    allocator: &mut PortAllocator,
    rpc_user: &str,
    rpc_pass: &str,
) -> Result<(
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Option<String>,
    Option<Vec<String>>,
)> {
    if let Some(config) = dynamic_app_config(app_id).await {
        return Ok(config);
    }

    Ok(match app_id {
        "homeassistant" | "home-assistant" => (
            vec!["8123:8123".to_string()],
            vec!["/var/lib/archipelago/home-assistant:/config".to_string()],
            vec!["TZ=UTC".to_string()],
            None,
            None,
        ),
        "bitcoin-core" => (
            vec![
                // RPC + ZMQ are auth-only/unauthenticated: host-local ONLY —
                // never the LAN. In-node consumers dial the container's
                // archy-net alias directly; never bind the archy-net gateway
                // (10.89.0.1) — rootlessport can't hold it and podman run
                // fails outright (2026-07-09, .228). P2P 8333 stays public.
                "127.0.0.1:8332:8332".to_string(),
                "8333:8333".to_string(),
                "127.0.0.1:28332:28332".to_string(),
                "127.0.0.1:28333:28333".to_string(),
            ],
            vec!["/var/lib/archipelago/bitcoin:/home/bitcoin/.bitcoin".to_string()],
            vec![],
            None,
            // Vanilla bitcoin/bitcoin image has no entrypoint wrapper and reads
            // only what's in bitcoin.conf + argv. The shared bitcoin.conf
            // carries rpcauth; we inject the networking flags as CLI args so
            // RPC is reachable from the bitcoin-ui companion container.
            //
            // Sync-speed flags:
            //   -dbcache     — UTXO set cache, sized to host RAM via
            //     bitcoin_dbcache_mb() (see there). A fixed 4GB cache swap-
            //     thrashed small nodes into fleet-wide 502s; ~1/16 of RAM
            //     keeps headroom for mempool + connections + the app stack.
            //   -par=0       — use all available cores for script
            //     verification (defaults to NCPU-1 capped at 16). Was
            //     effectively pinned at 2 by --cpus=2 (now removed).
            //   -maxconnections=125 — default but explicit, so ops can
            //     tune downward on bandwidth-constrained nodes.
            // Log volume: -printtoconsole=0 — bitcoind already writes
            // debug.log in the datadir (self-shrunk on restart); echoing it
            // to stdout too pushed every IBD "UpdateTip" line through
            // conmon into journald (>1 GB/day on a fresh node). Deep
            // debugging uses /var/lib/archipelago/bitcoin/debug.log.
            Some(vec![
                "-server=1".to_string(),
                "-rpcbind=0.0.0.0".to_string(),
                "-rpcallowip=0.0.0.0/0".to_string(),
                "-rpcport=8332".to_string(),
                "-printtoconsole=0".to_string(),
                "-datadir=/home/bitcoin/.bitcoin".to_string(),
                format!("-dbcache={}", bitcoin_dbcache_mb()),
                "-par=0".to_string(),
                "-maxconnections=125".to_string(),
            ]),
        ),
        "bitcoin" | "bitcoin-knots" => (
            vec![
                // RPC + ZMQ are auth-only/unauthenticated: host-local ONLY —
                // never the LAN. In-node consumers dial the container's
                // archy-net alias directly; never bind the archy-net gateway
                // (10.89.0.1) — rootlessport can't hold it and podman run
                // fails outright (2026-07-09, .228). P2P 8333 stays public.
                "127.0.0.1:8332:8332".to_string(),
                "8333:8333".to_string(),
                "127.0.0.1:28332:28332".to_string(),
                "127.0.0.1:28333:28333".to_string(),
            ],
            vec!["/var/lib/archipelago/bitcoin:/home/bitcoin/.bitcoin".to_string()],
            vec![],
            None,
            None,
        ),
        "lnd" => (
            vec![
                "9735:9735".to_string(),
                "10009:10009".to_string(),
                "18080:8080".to_string(),
            ],
            vec!["/var/lib/archipelago/lnd:/root/.lnd".to_string()],
            vec![],
            None,
            Some(vec![
                "--bitcoin.active".to_string(),
                "--bitcoin.mainnet".to_string(),
                "--bitcoin.node=bitcoind".to_string(),
                format!("--bitcoind.rpcuser={}", rpc_user),
                format!("--bitcoind.rpcpass={}", rpc_pass),
                "--bitcoind.rpchost=bitcoin-knots:8332".to_string(),
                "--rpclisten=0.0.0.0:10009".to_string(),
                "--restlisten=0.0.0.0:8080".to_string(),
                "--listen=0.0.0.0:9735".to_string(),
            ]),
        ),
        "btcpay-server" | "btcpayserver" => (
            vec!["23000:49392".to_string()],
            vec!["/var/lib/archipelago/btcpay:/datadir".to_string()],
            vec![
                "ASPNETCORE_URLS=http://0.0.0.0:49392".to_string(),
                "BTCPAY_PROTOCOL=http".to_string(),
                format!("BTCPAY_HOST={}:23000", host_ip),
                "BTCPAY_CHAINS=btc".to_string(),
                "BTCPAY_BTCEXPLORERURL=http://archy-nbxplorer:32838".to_string(),
                "BTCPAY_BTCRPCURL=http://bitcoin-knots:8332".to_string(),
                format!("BTCPAY_BTCRPCUSER={}", rpc_user),
                format!("BTCPAY_BTCRPCPASSWORD={}", rpc_pass),
                format!("BTCPAY_POSTGRES=User ID=btcpay;Password={};Host=archy-btcpay-db;Port=5432;Database=btcpay;Include Error Detail=true",
                    read_secret("btcpay-db-password", "btcpaypass")),
            ],
            None,
            None,
        ),
        "mempool" | "mempool-web" => (
            vec!["4080:8080".to_string()],
            vec![],
            vec!["BACKEND_MAINNET_HTTP_HOST=mempool-api".to_string()],
            None,
            None,
        ),
        "mempool-api" => {
            // CORE_RPC_HOST must resolve to the actual Bitcoin node container —
            // bitcoin-knots OR bitcoin-core — else mempool-api can't reach RPC
            // on a Core node (B12). Falls back to bitcoin-knots if undetected.
            let bitcoin_rpc_host = super::dependencies::detect_bitcoin_rpc_host().await;
            (
                vec!["8999:8999".to_string()],
                vec!["/var/lib/archipelago/mempool:/data".to_string()],
                vec![
                    "MEMPOOL_BACKEND=electrum".to_string(),
                    "ELECTRUM_HOST=electrumx".to_string(),
                    "ELECTRUM_PORT=50001".to_string(),
                    "ELECTRUM_TLS_ENABLED=false".to_string(),
                    format!("CORE_RPC_HOST={}", bitcoin_rpc_host),
                    "CORE_RPC_PORT=8332".to_string(),
                    "CORE_RPC_USERNAME=archipelago".to_string(),
                    format!("CORE_RPC_PASSWORD={}", rpc_pass),
                    "DATABASE_ENABLED=true".to_string(),
                    "DATABASE_HOST=archy-mempool-db".to_string(),
                    "DATABASE_DATABASE=mempool".to_string(),
                    "DATABASE_USERNAME=mempool".to_string(),
                    format!("DATABASE_PASSWORD={}", read_secret("mempool-db-password", "mempoolpass")),
                ],
                None,
                None,
            )
        }
        "electrumx" | "mempool-electrs" | "electrs" => {
            (
                vec!["50001:50001".to_string()],
                vec!["/var/lib/archipelago/electrumx:/data".to_string()],
                vec![
                    format!(
                        "DAEMON_URL=http://{}:{}@bitcoin-knots:8332/",
                        rpc_user, rpc_pass
                    ),
                    "COIN=Bitcoin".to_string(),
                    "DB_DIRECTORY=/data".to_string(),
                    "SERVICES=tcp://:50001,rpc://0.0.0.0:8000".to_string(),
                    // Keep cache below the container limit; high values
                    // have caused OOM/restart loops during catch-up.
                    "CACHE_MB=1024".to_string(),
                    // Block-fetcher concurrency — defaults are conservative
                    // for shared hosts; 4 is plenty for one bitcoind backend.
                    "MAX_SEND=10000000".to_string(),
                ],
                None,
                None,
            )
        }
        "mysql-mempool" => (
            vec![],
            vec!["/var/lib/archipelago/mysql-mempool:/var/lib/mysql".to_string()],
            vec![
                "MYSQL_DATABASE=mempool".to_string(),
                "MYSQL_USER=mempool".to_string(),
                format!("MYSQL_PASSWORD={}", read_secret("mempool-db-password", "mempoolpass")),
                format!("MYSQL_ROOT_PASSWORD={}", read_secret("mysql-root-db-password", "rootpass")),
            ],
            None,
            None,
        ),
        "grafana" => (
            vec!["3000:3000".to_string()],
            vec!["/var/lib/archipelago/grafana:/var/lib/grafana".to_string()],
            vec![
                "GF_PATHS_DATA=/var/lib/grafana".to_string(),
                "GF_USERS_ALLOW_SIGN_UP=false".to_string(),
            ],
            None,
            None,
        ),
        "searxng" => (
            vec!["8888:8080".to_string()],
            vec!["/var/lib/archipelago/searxng:/etc/searxng".to_string()],
            vec![],
            None,
            None,
        ),
        "ollama" => (
            vec!["11434:11434".to_string()],
            vec!["/var/lib/archipelago/ollama:/root/.ollama".to_string()],
            vec![],
            None,
            None,
        ),
        "cryptpad" => (
            vec!["3003:3000".to_string()],
            vec!["/var/lib/archipelago/cryptpad:/cryptpad/datastore".to_string()],
            vec![],
            None,
            None,
        ),
        "penpot" | "penpot-frontend" => (
            vec!["9001:80".to_string()],
            vec![],
            vec![],
            None,
            None,
        ),
        "nextcloud" => {
            let host_port = allocator
                .allocate_or_get(app_id, 8085, 80)
                .await
                .unwrap_or(8085);
            (
                vec![format!("{}:80", host_port)],
                vec!["/var/lib/archipelago/nextcloud:/var/www/html".to_string()],
                vec![],
                None,
                None,
            )
        }
        "vaultwarden" => {
            let host_port = allocator
                .allocate_or_get(app_id, 8082, 80)
                .await
                .unwrap_or(8082);
            (
                vec![format!("{}:80", host_port)],
                vec!["/var/lib/archipelago/vaultwarden:/data".to_string()],
                vec![],
                None,
                None,
            )
        }
        "jellyfin" => (
            vec!["8096:8096".to_string()],
            vec![
                "/var/lib/archipelago/jellyfin/config:/config".to_string(),
                "/var/lib/archipelago/jellyfin/cache:/cache".to_string(),
            ],
            vec![],
            None,
            None,
        ),
        "photoprism" => (
            vec!["2342:2342".to_string()],
            vec!["/var/lib/archipelago/photoprism:/photoprism/storage".to_string()],
            vec![
                "PHOTOPRISM_ADMIN_PASSWORD=archipelago".to_string(),
                "PHOTOPRISM_DEFAULT_LOCALE=en".to_string(),
            ],
            None,
            None,
        ),
        "immich" => (
            vec!["2283:2283".to_string()],
            vec!["/var/lib/archipelago/immich:/usr/src/app/upload".to_string()],
            vec![
                "DB_HOSTNAME=immich_postgres".to_string(),
                "DB_USERNAME=postgres".to_string(),
                format!("DB_PASSWORD={}", read_secret("immich-db-password", "immichpass")),
                "DB_DATABASE_NAME=immich".to_string(),
                "REDIS_HOSTNAME=immich_redis".to_string(),
                "UPLOAD_LOCATION=/usr/src/app/upload".to_string(),
            ],
            None,
            None,
        ),
        "filebrowser" => {
            let host_port = allocator
                .allocate_or_get(app_id, 8083, 80)
                .await
                .unwrap_or(8083);
            (
                vec![format!("{}:80", host_port)],
                vec![
                    "/var/lib/archipelago/filebrowser:/srv".to_string(),
                    "/var/lib/archipelago/filebrowser-data:/data".to_string(),
                ],
                vec![],
                None,
                Some(vec![
                    "--database=/data/database.db".to_string(),
                    "--root=/srv".to_string(),
                    "--address=0.0.0.0".to_string(),
                    "--port=80".to_string(),
                ]),
            )
        }
        "nginx-proxy-manager" => {
            let storage = crate::container::npm::resolve_storage().await?;
            let admin_port = allocator
                .allocate_or_get(app_id, 8081, 81)
                .await
                .unwrap_or(8081);
            let http_port = allocator
                .allocate_or_get("nginx-proxy-manager-http", 8088, 80)
                .await
                .unwrap_or(8088);
            let https_port = allocator
                .allocate_or_get("nginx-proxy-manager-https", 8444, 443)
                .await
                .unwrap_or(8444);
            (
                vec![
                    format!("127.0.0.1:{}:81", admin_port),
                    format!("127.0.0.1:{}:80", http_port),
                    format!("127.0.0.1:{}:443", https_port),
                ],
                storage.bind_mounts(),
                vec![],
                None,
                None,
            )
        }
        "portainer" => (
            vec!["9000:9000".to_string()],
            vec![
                "/var/lib/archipelago/portainer:/data".to_string(),
                "/run/user/1000/podman/podman.sock:/var/run/docker.sock".to_string(),
                "/var/lib/archipelago/portainer/compose:/data/compose".to_string(),
            ],
            vec![],
            None,
            None,
        ),
        "uptime-kuma" => (
            vec!["3002:3001".to_string()],
            vec!["/var/lib/archipelago/uptime-kuma:/app/data".to_string()],
            vec!["TZ=UTC".to_string()],
            None,
            Some(vec![
                "--".to_string(),
                "node".to_string(),
                "server/server.js".to_string(),
            ]),
        ),
        "tailscale" => (
            vec!["8240:8240".to_string()],
            vec!["/var/lib/archipelago/tailscale:/var/lib/tailscale".to_string()],
            vec!["TS_STATE_DIR=/var/lib/tailscale".to_string()],
            // Don't use custom_command (Option<String>) — install.rs passes
            // it as a SINGLE arg to podman, which then treats the whole
            // "sh -c 'tailscale web …'" string as the executable name and
            // fails: "executable file `sh -c 'tailscale web …'` not found".
            // custom_args (Option<Vec<String>>) splits properly.
            None,
            Some(vec![
                "sh".to_string(),
                "-c".to_string(),
                "tailscaled --tun=userspace-networking & for i in $(seq 1 30); do [ -S /var/run/tailscale/tailscaled.sock ] && break; sleep 1; done; tailscale web --listen 0.0.0.0:8240 & wait".to_string(),
            ]),
        ),
        "fedimint" => (
            vec![
                "8173:8173".to_string(),
                "8174:8174".to_string(),
                "8175:8175".to_string(),
            ],
            vec!["/var/lib/archipelago/fedimint:/data".to_string()],
            vec![
                "FM_DATA_DIR=/data".to_string(),
                format!("FM_BITCOIND_USERNAME={}", rpc_user),
                format!("FM_BITCOIND_PASSWORD={}", rpc_pass),
                "FM_BITCOIN_NETWORK=bitcoin".to_string(),
                "FM_BIND_P2P=0.0.0.0:8173".to_string(),
                "FM_BIND_API=0.0.0.0:8174".to_string(),
                "FM_BIND_UI=0.0.0.0:8175".to_string(),
                format!("FM_P2P_URL=fedimint://{}:8173", host_ip),
                format!("FM_API_URL=ws://{}:8174", host_ip),
                "FM_BITCOIND_URL=http://bitcoin-knots:8332".to_string(),
            ],
            None,
            Some(vec![
                "--data-dir".to_string(),
                "/data".to_string(),
                format!("--bitcoind-url=http://{}:{}@bitcoin-knots:8332", rpc_user, rpc_pass),
            ]),
        ),
        "fedimint-gateway" => {
            // FED-07: no fallback literal. A fresh install self-provisions its
            // own credential; a node that can't obtain one fails this install
            // outright (propagated via `?`) rather than start an
            // unauthenticated/default-credentialed gateway.
            let gateway_secrets_dir = std::path::Path::new("/var/lib/archipelago/secrets");
            crate::container::secrets::ensure_gateway_credential(gateway_secrets_dir)?;
            let fedi_hash = crate::container::secrets::gateway_bcrypt_hash(gateway_secrets_dir)?;
            (
                vec!["8176:8176".to_string(), "9737:9737".to_string()],
                vec!["/var/lib/archipelago/fedimint-gateway:/data".to_string()],
                vec![],
                None,
                Some(vec![
                    "gatewayd".to_string(),
                    "--data-dir".to_string(),
                    "/data".to_string(),
                    "--listen".to_string(),
                    "0.0.0.0:8176".to_string(),
                    "--bcrypt-password-hash".to_string(),
                    fedi_hash,
                    "--network".to_string(),
                    "bitcoin".to_string(),
                    "--bitcoind-url".to_string(),
                    "http://bitcoin-knots:8332".to_string(),
                    "--bitcoind-username".to_string(),
                    rpc_user.to_string(),
                    "--bitcoind-password".to_string(),
                    rpc_pass.to_string(),
                    "ldk".to_string(),
                    "--ldk-lightning-port".to_string(),
                    "9737".to_string(),
                    "--ldk-alias".to_string(),
                    "archipelago-gateway".to_string(),
                ]),
            )
        }
        "indeedhub" => (
            vec!["7778:7777".to_string()],
            vec![],
            vec![
                "NODE_ENV=production".to_string(),
                "NEXT_TELEMETRY_DISABLED=1".to_string(),
            ],
            None,
            None,
        ),
        "nostr-rs-relay" => (
            vec!["18081:8080".to_string()],
            vec!["/var/lib/archipelago/nostr-rs-relay:/usr/src/app/db".to_string()],
            vec![],
            None,
            None,
        ),
        "routstr" => {
            let nsec = read_nostr_secret_hex();
            let mut env = vec![
                "DATABASE_URL=sqlite:///app/data/keys.db".to_string(),
            ];
            if !nsec.is_empty() {
                env.push(format!("NSEC={}", nsec));
                env.push(format!("NOSTR_PUBKEY={}", read_nostr_pubkey_hex()));
            }
            (
                vec!["8200:8000".to_string()],
                vec!["/var/lib/archipelago/routstr:/app/data".to_string()],
                env,
                None,
                None,
            )
        }
        "nostr-vpn" => {
            let nsec = read_nostr_secret_hex();
            let mut env = vec![];
            if !nsec.is_empty() {
                env.push(format!("NOSTR_SECRET={}", nsec));
                env.push(format!("NOSTR_PUBKEY={}", read_nostr_pubkey_hex()));
            }
            (
                vec!["51820:51820/udp".to_string()],
                vec!["/var/lib/archipelago/nostr-vpn:/root/.config/nvpn".to_string()],
                env,
                None,
                None,
            )
        }
        "fips" => {
            let nsec = read_nostr_secret_hex();
            let mut env = vec![];
            if !nsec.is_empty() {
                env.push(format!("FIPS_NSEC={}", nsec));
                env.push(format!("FIPS_NPUB={}", read_nostr_pubkey_hex()));
            }
            (
                vec![
                    "2121:2121/udp".to_string(),
                    "8443:8443".to_string(),
                ],
                vec![
                    "/var/lib/archipelago/fips/config:/etc/fips".to_string(),
                    "/var/lib/archipelago/fips/run:/run/fips".to_string(),
                ],
                env,
                None,
                None,
            )
        }
        "botfights" => {
            let jwt_secret = read_or_generate_secret("botfights-jwt").await;
            (
                vec!["9100:9100".to_string()],
                vec!["/var/lib/archipelago/botfights:/app/server/data".to_string()],
                vec![
                    "NODE_ENV=production".to_string(),
                    "PORT=9100".to_string(),
                    format!("JWT_SECRET={}", jwt_secret),
                    "FIGHT_LOOP_ENABLED=true".to_string(),
                    "ARCHY_EMBEDDED=1".to_string(),
                ],
                None,
                None,
            )
        }
        // Gitea listens on container port 3000 and is launched directly on
        // host port 3001 because it blocks iframe embedding.
        "gitea" => (
            vec!["3001:3000".to_string(), "2222:22".to_string()],
            vec![
                "/var/lib/archipelago/gitea/data:/data".to_string(),
                "/var/lib/archipelago/gitea/config:/etc/gitea".to_string(),
            ],
            vec![
                "GITEA__database__DB_TYPE=sqlite3".to_string(),
                "GITEA__server__SSH_PORT=2222".to_string(),
                "GITEA__server__SSH_LISTEN_PORT=22".to_string(),
                "GITEA__server__LFS_START_SERVER=true".to_string(),
                "GITEA__packages__ENABLED=true".to_string(),
                "GITEA__repository__ENABLE_PUSH_CREATE_USER=true".to_string(),
                "GITEA__repository__ENABLE_PUSH_CREATE_ORG=true".to_string(),
                "GITEA__security__X_FRAME_OPTIONS=".to_string(),
            ],
            None,
            None,
        ),
        _ => {
            // No catalog runtime metadata found; use minimal defaults
            // (container's own EXPOSE/VOLUME). New generic apps should declare
            // containerConfig in the registry catalog instead of adding Rust cases.
            tracing::warn!("No catalog runtime config found for app: {} — using minimal defaults", app_id);
            (vec![], vec![], vec![], None, None)
        }
    })
}
