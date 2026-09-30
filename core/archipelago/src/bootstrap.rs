//! Bootstrap host-side artifacts on every archipelago startup.
//!
//! The update pipeline swaps the archipelago binary but does not touch
//! scripts, systemd units, or nginx configuration — those are installed
//! once by the ISO builder. Without this module, changes to
//! `container-doctor.sh`, the doctor service/timer, or the nginx config
//! never reach boxes installed before the change.
//!
//! Two things are synced on startup:
//!   1. Doctor artifacts (container-doctor.sh + service + timer).
//!   2. Missing nginx backend proxy blocks required for frontend fetches to
//!      reach the backend instead of the SPA fallback.
//!
//! Idempotent: no-ops on boxes that are already in sync. All work is
//! best-effort — failures are logged but never abort the backend.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

use crate::update::host_sudo;

const DOCTOR_SH: &str = include_str!("../../../scripts/container-doctor.sh");
const DOCTOR_SERVICE: &str =
    include_str!("../../../image-recipe/configs/archipelago-doctor.service");
const DOCTOR_TIMER: &str = include_str!("../../../image-recipe/configs/archipelago-doctor.timer");

const DOCTOR_SH_PATH: &str = "/opt/archipelago/scripts/container-doctor.sh";
const DOCTOR_SERVICE_PATH: &str = "/etc/systemd/system/archipelago-doctor.service";
const DOCTOR_TIMER_PATH: &str = "/etc/systemd/system/archipelago-doctor.timer";

// Kiosk hardening (#36): keep the deployed unit + launcher in sync with the
// repo so the CPU/memory cap and the GPU-vs-headless flag selection reach
// already-installed nodes via OTA, not just fresh ISOs.
const KIOSK_SERVICE: &str = include_str!("../../../image-recipe/configs/archipelago-kiosk.service");
const KIOSK_LAUNCHER: &str =
    include_str!("../../../image-recipe/configs/archipelago-kiosk-launcher.sh");
const KIOSK_SERVICE_PATH: &str = "/etc/systemd/system/archipelago-kiosk.service";
const KIOSK_LAUNCHER_PATH: &str = "/usr/local/bin/archipelago-kiosk-launcher";

// HDMI audio (kiosk nodes): ISOs built before 2026-07-23 shipped no audio
// stack at all even though the kiosk launcher expects PipeWire-Pulse, and the
// kiosk's boot-time modeset can race the i915→HDA audio-component bind so the
// HDMI ELD is lost and every HDMI profile stays unavailable (silent failure).
// The router daemon handles routing + the ELD re-modeset nudge; this heal
// installs the packages, group membership, script and unit on deployed nodes.
const AUDIO_ROUTER: &str =
    include_str!("../../../image-recipe/configs/archipelago-audio-router.sh");
const AUDIO_SERVICE: &str =
    include_str!("../../../image-recipe/configs/archipelago-audio-router.service");
const AUDIO_ROUTER_PATH: &str = "/usr/local/bin/archipelago-audio-router";
const AUDIO_SERVICE_PATH: &str = "/etc/systemd/system/archipelago-audio-router.service";

// Gamepad→keyboard bridge (TV input inside every app iframe) — same
// splice-from-configs + self-heal pattern as the audio router.
const GAMEPAD_KEYS: &str =
    include_str!("../../../image-recipe/configs/archipelago-gamepad-keys.py");
const GAMEPAD_SERVICE: &str =
    include_str!("../../../image-recipe/configs/archipelago-gamepad-keys.service");
const GAMEPAD_KEYS_PATH: &str = "/usr/local/bin/archipelago-gamepad-keys";
const GAMEPAD_SERVICE_PATH: &str = "/etc/systemd/system/archipelago-gamepad-keys.service";

// Journald log-volume policy (size cap + per-service rate limit). Fresh ISOs
// write the identical file at build time (image-recipe/_archived/
// build-auto-installer-iso.sh); this heals already-deployed nodes via OTA.
// A fresh node produced >1 GB/day of journal (bitcoind IBD console spam plus
// debug-level backend logging) — the cap bounds disk use and the rate limit
// keeps one chatty service from drowning everything else.
const JOURNALD_DROPIN: &str =
    include_str!("../../../image-recipe/configs/journald-archipelago.conf");
const JOURNALD_DROPIN_PATH: &str = "/etc/systemd/journald.conf.d/10-archipelago-persistent.conf";

const NGINX_CONF_PATH: &str = "/etc/nginx/sites-available/archipelago";
const NGINX_ENABLED_CONF_PATH: &str = "/etc/nginx/sites-enabled/archipelago";
/// Per-app proxy snippet included by the HTTPS (:443) server block. Carries its
/// own `/app/fedimint/` location, so it needs the same B13 asset-rewrite heal as
/// the main conf — browsers reach fedimint over HTTPS via this snippet. Absent on
/// HTTP-only nodes, in which case the bootstrap loop skips it.
const NGINX_HTTPS_SNIPPET_PATH: &str = "/etc/nginx/snippets/archipelago-https-app-proxies.conf";
const RUNTIME_ASSETS_DIR: &str = "/opt/archipelago/web-ui/archipelago-runtime";

/// Inserted into every server block of the nginx config that lacks the
/// `/api/app-catalog` proxy. Kept in sync with the canonical block in
/// image-recipe/configs/nginx-archipelago.conf.
const NGINX_APP_CATALOG_BLOCK: &str = "\n    # App Store catalog proxy — backend fetches from configured registries\n    # so the browser doesn't hit CORS/CSP. Without this block nginx falls\n    # through to the SPA index.html and the frontend gets HTML back instead\n    # of JSON.\n    location /api/app-catalog {\n        proxy_pass http://127.0.0.1:5678;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header Cookie $http_cookie;\n        proxy_connect_timeout 15s;\n        proxy_read_timeout 30s;\n        proxy_send_timeout 15s;\n        error_page 502 503 = @backend_unavailable;\n        error_page 504 = @backend_timeout;\n    }\n\n";

const NGINX_SOURCE_PROXY_BLOCK: &str = "    # GitWorkshop follows the dashboard origin so LAN, Tailscale, FIPS, Tor,\n    # hostnames and reverse proxies all use the connection that already works.\n    location /app/archipelago-source/ {\n        proxy_pass http://127.0.0.2:8337/;\n        proxy_http_version 1.1;\n        proxy_set_header Host $http_host;\n        proxy_set_header Cookie $http_cookie;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_set_header X-Forwarded-Prefix /app/archipelago-source;\n        proxy_hide_header X-Frame-Options;\n        add_header X-Frame-Options \"SAMEORIGIN\" always;\n        add_header X-Content-Type-Options \"nosniff\" always;\n        proxy_read_timeout 300s;\n    }\n";

const NGINX_SOURCE_PROXY_BLOCK_SNIPPET: &str = "# GitWorkshop follows the dashboard origin; the app gate keeps the route\n# session-authenticated before it reaches the loopback-only container.\nlocation /app/archipelago-source/ {\n    proxy_pass http://127.0.0.2:8337/;\n    proxy_http_version 1.1;\n    proxy_set_header Host $http_host;\n    proxy_set_header Cookie $http_cookie;\n    proxy_set_header X-Real-IP $remote_addr;\n    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n    proxy_set_header X-Forwarded-Proto $scheme;\n    proxy_set_header X-Forwarded-Prefix /app/archipelago-source;\n    proxy_hide_header X-Frame-Options;\n    add_header X-Frame-Options \"SAMEORIGIN\" always;\n    add_header X-Content-Type-Options \"nosniff\" always;\n    proxy_read_timeout 300s;\n}\n";

/// The normal dashboard sends X-Frame-Options SAMEORIGIN. This one document
/// must be frameable by an app on another port of the same node so tabs and
/// companion WebViews can use the same authenticated signer UI.
const NGINX_NOSTR_SIGNER_BLOCK: &str = "    # Dashboard-origin Nostr signer for tab/WebView apps.\n    location = /nostr-signer {\n        try_files /index.html =404;\n        add_header Cache-Control \"no-store\" always;\n        add_header X-Content-Type-Options \"nosniff\" always;\n        add_header Referrer-Policy \"no-referrer\" always;\n        add_header Content-Security-Policy \"default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; frame-ancestors 'self' http://$host:* https://$host:*; base-uri 'none'; form-action 'none';\" always;\n    }\n\n";

const NGINX_BITCOIN_STATUS_BLOCK: &str = "\n    location /bitcoin-status {\n        proxy_pass http://127.0.0.1:5678/bitcoin-status;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_connect_timeout 10s;\n        proxy_read_timeout 10s;\n        proxy_send_timeout 5s;\n        error_page 502 503 = @backend_unavailable;\n        error_page 504 = @backend_timeout;\n    }\n";

/// Inserted into every server block that lacks the `/proxy/lnd/` proxy. Nodes
/// flashed before 2026-04-10 shipped an nginx config without this block, so the
/// browser's wallet fetches to `/proxy/lnd/*` fell through to the SPA
/// index.html and got HTML back instead of JSON ("failing to fetch"). Kept in
/// sync with the canonical block in image-recipe/configs/nginx-archipelago.conf.
const NGINX_LND_PROXY_BLOCK: &str = "\n    # LND REST proxy — backend handles auth + CORS\n    location /proxy/lnd/ {\n        proxy_pass http://127.0.0.1:5678;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header Cookie $http_cookie;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_connect_timeout 10s;\n        proxy_read_timeout 10s;\n        proxy_send_timeout 5s;\n        error_page 502 503 = @backend_unavailable;\n        error_page 504 = @backend_timeout;\n    }\n";

/// Inserted into every server block lacking the peer-content streaming proxy.
/// Without it, the browser's `<video>`/`<audio>` Range requests to
/// `/api/peer-content/*` fall through to the SPA index.html (HTML, no Range)
/// and peer media won't play (B3). Forwards Cookie (session auth) + Range and
/// disables buffering so streaming works. Kept in sync with the canonical
/// block in image-recipe/configs/nginx-archipelago.conf.
const NGINX_PEER_CONTENT_BLOCK: &str = "\n    # Peer content streaming proxy (B3) — Range-streams a peer's media file.\n    # Long read timeout: this path also serves full-file downloads of large\n    # media (#38), which can take minutes over Tor; 120s aborted them.\n    location /api/peer-content/ {\n        proxy_pass http://127.0.0.1:5678;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header Cookie $http_cookie;\n        proxy_set_header Range $http_range;\n        proxy_buffering off;\n        proxy_connect_timeout 10s;\n        proxy_read_timeout 900s;\n        error_page 502 503 = @backend_unavailable;\n        error_page 504 = @backend_timeout;\n    }\n";

/// Inserted into every server block lacking the Pine node-status proxy.
/// `/api/pine/status` serves the Pine launcher page's live status card and
/// the seeded Home Assistant REST sensors (the sensitive tier is gated by a
/// bearer token at the backend, so nginx just forwards). Kept in sync with
/// the canonical block in image-recipe/configs/nginx-archipelago.conf.
const NGINX_PINE_STATUS_BLOCK: &str = "\n    # Pine node status — live node facts for the Pine launcher page and the\n    # seeded Home Assistant sensors. Sensitive fields are token-gated at the\n    # backend; nginx only forwards.\n    location /api/pine/status {\n        proxy_pass http://127.0.0.1:5678;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header Authorization $http_authorization;\n        proxy_set_header Cookie $http_cookie;\n        proxy_connect_timeout 10s;\n        proxy_read_timeout 15s;\n        proxy_send_timeout 5s;\n        error_page 502 503 = @backend_unavailable;\n        error_page 504 = @backend_timeout;\n    }\n";

/// B13 — Fedimint UI asset rewrite. Pre-fix nodes proxy /app/fedimint/ with only
/// the nostr-provider injection (`sub_filter_once on`), so the UI's root-rooted
/// CSS/JS asset URLs (href="/…", url("/…")) miss the proxy and load the SPA shell
/// → unstyled UI. We swap that single sub_filter for the full rewrite set that
/// reroots every asset URL under /app/fedimint/. NEW matches the canonical block
/// in image-recipe/configs/nginx-archipelago.conf byte-for-byte so self-healed
/// nodes converge to the same config fresh ISOs ship with.
const NGINX_FEDIMINT_OLD: &str = "        sub_filter_once on;\n        sub_filter '</head>' '<script src=\"/nostr-provider.js\"></script></head>';\n    }\n    location /app/fedimint-gateway/ {";
const NGINX_FEDIMINT_NEW: &str = "        sub_filter_types text/css application/javascript application/json;\n        sub_filter_once off;\n        sub_filter 'href=\"/' 'href=\"/app/fedimint/';\n        sub_filter 'src=\"/' 'src=\"/app/fedimint/';\n        sub_filter \"href='/\" \"href='/app/fedimint/\";\n        sub_filter \"src='/\" \"src='/app/fedimint/\";\n        sub_filter 'url(\"/' 'url(\"/app/fedimint/';\n        sub_filter \"url('/\" \"url('/app/fedimint/\";\n        sub_filter '</head>' '<script src=\"/nostr-provider.js\"></script></head>';\n    }\n    location /app/fedimint-gateway/ {";

/// B13 Style B — the HTTPS app-proxy snippet's fedimint block has NO sub_filter
/// at all (older than the main conf's), and the directive that follows it varies
/// per node (fedimint-gateway vs tailscale), so a full-block match is unreliable.
/// Instead we anchor on the unique :8175 proxy_pass (fedimint is the only block
/// proxying there) and insert the reroot set right after it — directive order
/// inside a location block is irrelevant to nginx. Idempotent via the same
/// `href="/app/fedimint/` marker the main-conf heal leaves behind.
const NGINX_FEDIMINT_SNIPPET_ANCHOR: &str = "proxy_pass http://127.0.0.1:8175/;";
const NGINX_FEDIMINT_SNIPPET_INSERT: &str = "proxy_pass http://127.0.0.1:8175/;\n    proxy_set_header Accept-Encoding \"\";\n    sub_filter_types text/css application/javascript application/json;\n    sub_filter_once off;\n    sub_filter 'href=\"/' 'href=\"/app/fedimint/';\n    sub_filter 'src=\"/' 'src=\"/app/fedimint/';\n    sub_filter \"href='/\" \"href='/app/fedimint/\";\n    sub_filter \"src='/\" \"src='/app/fedimint/\";\n    sub_filter 'url(\"/' 'url(\"/app/fedimint/';\n    sub_filter \"url('/\" \"url('/app/fedimint/\";\n    sub_filter '</head>' '<script src=\"/nostr-provider.js\"></script></head>';";

/// Finish manifest promotion before constructing the orchestrator or starting
/// catalog refresh/reconciliation. Replacing the app tree in the background
/// could let a reload observe its temporary empty state and forget disk-only apps.
pub async fn ensure_runtime_assets_ready() {
    match run_runtime_assets().await {
        Ok(changed) if changed => info!("Runtime assets synchronized from OTA payload"),
        Ok(_) => debug!("No OTA runtime payload to synchronize"),
        Err(e) => warn!("Runtime asset bootstrap failed (non-fatal): {:#}", e),
    }
    // Repair the narrowly recognized legacy NPM tunnel override before app
    // reconciliation. The embedded script ships in both OTA and ISO binaries.
    // It preserves native wallet services and refuses unknown custom routing.
    match tokio::process::Command::new("python3")
        .arg("-c")
        .arg(include_str!("../../../scripts/repair-npm-tunnel.py"))
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            if !output.stdout.is_empty() {
                info!("{}", String::from_utf8_lossy(&output.stdout).trim());
            }
        }
        Ok(output) => warn!(
            "NPM tunnel migration needs attention: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => warn!("NPM tunnel migration could not run: {error}"),
    }
    match run_apps_dir_repair().await {
        Ok(true) => {
            info!("Populated /opt/archipelago/apps from installer copy at /etc/archipelago/apps")
        }
        Ok(false) => debug!("/opt/archipelago/apps already populated (or no installer copy)"),
        Err(e) => warn!("Apps dir repair failed (non-fatal): {:#}", e),
    }
}

/// Entry point called from main startup. Never returns an error to the caller —
/// failing to bootstrap host artifacts must not prevent the backend from serving.
pub async fn ensure_doctor_installed() {
    match run_service_override_repair().await {
        Ok(true) => info!("Removed stale Archipelago dev-mode service override"),
        Ok(false) => debug!("No stale Archipelago dev-mode service override found"),
        Err(e) => warn!("Service override repair failed (non-fatal): {:#}", e),
    }
    match run().await {
        Ok(changed) if changed => info!("Doctor artifacts synchronized with binary"),
        Ok(_) => debug!("Doctor artifacts already in sync"),
        Err(e) => warn!("Doctor bootstrap failed (non-fatal): {:#}", e),
    }
    match run_nginx().await {
        Ok(true) => info!("Patched nginx config to proxy missing backend endpoints"),
        Ok(false) => debug!("Nginx backend endpoint proxy blocks already present"),
        Err(e) => warn!("Nginx bootstrap failed (non-fatal): {:#}", e),
    }
    match run_bitcoin_rpc_repair().await {
        Ok(true) => {
            info!("Removed stale bitcoin.conf; running Bitcoin containers left untouched")
        }
        Ok(false) => debug!("No stale bitcoin.conf found"),
        Err(e) => warn!("Bitcoin RPC repair failed (non-fatal): {:#}", e),
    }
    match run_tor_helper_sync().await {
        Ok(true) => info!("tor-helper.sh synchronized with binary"),
        Ok(false) => debug!("tor-helper.sh already current"),
        Err(e) => warn!("tor-helper sync failed (non-fatal): {:#}", e),
    }
    match run_welcome_banner_sync().await {
        Ok(true) => info!(
            "Console welcome banner synchronized (LAN address + .local name, not the WG tunnel IP)"
        ),
        Ok(false) => debug!("Console welcome banner already current (or not an ISO node)"),
        Err(e) => warn!("Welcome banner sync failed (non-fatal): {:#}", e),
    }
    match run_nginx_listener_repair().await {
        Ok(true) => info!("nginx HTTPS listeners retargeted to this host's current addresses"),
        Ok(false) => debug!("nginx listeners already match this host's addresses"),
        Err(e) => warn!("nginx listener repair failed (non-fatal): {:#}", e),
    }
    match run_ha_rpc_proxy_bind_repair().await {
        Ok(true) => {
            info!("HA bitcoind RPC forwarder rebound dynamically — survives network moves now")
        }
        Ok(false) => debug!("HA bitcoind RPC forwarder absent or already dynamic"),
        Err(e) => warn!("HA RPC forwarder bind repair failed (non-fatal): {:#}", e),
    }
    match run_pull_never_image_repair().await {
        Ok(n) if n > 0 => info!(
            retagged = n,
            "Healed quadlet image refs orphaned by registry rename"
        ),
        Ok(_) => debug!("All quadlet image refs resolve locally"),
        Err(e) => warn!("Quadlet image ref repair failed (non-fatal): {:#}", e),
    }
    match run_tor_torrc_repair().await {
        Ok(true) => info!("Tor healed at boot (torrc rebuilt and/or daemon restarted)"),
        Ok(false) => debug!("Tor healthy and torrc in sync — no heal needed"),
        Err(e) => warn!("Tor boot heal failed (non-fatal): {:#}", e),
    }
    match run_nginx_mempool_ws_repair().await {
        Ok(true) => info!("nginx mempool websocket headers repaired and reloaded"),
        Ok(false) => debug!("nginx mempool websocket headers already present"),
        Err(e) => warn!("nginx mempool ws repair failed (non-fatal): {:#}", e),
    }
    match run_polkit_networkmanager_repair().await {
        Ok(true) => info!(
            "Installed NetworkManager polkit rule for the archipelago user — Wi-Fi setup enabled"
        ),
        Ok(false) => debug!("NetworkManager polkit rule already present"),
        Err(e) => warn!("polkit NetworkManager repair failed (non-fatal): {:#}", e),
    }
    match run_journald_dropin().await {
        Ok(true) => info!("Installed journald log-volume policy drop-in"),
        Ok(false) => debug!("journald log-volume policy already in place"),
        Err(e) => warn!("journald drop-in bootstrap failed (non-fatal): {:#}", e),
    }
    match tighten_secrets_dir().await {
        Ok(n) if n > 0 => info!(tightened = n, "Tightened mode on secret files"),
        Ok(_) => debug!("Secrets directory already at expected mode"),
        Err(e) => warn!("Secrets dir tightening failed (non-fatal): {:#}", e),
    }
    // Podman probing MUST be the last bootstrap stage. We used to delete
    // transient runroot state here when `podman info` failed, but live nodes
    // can still have rootlessport/conmon processes holding that state. Removing
    // it automatically makes failures worse: containers lose `.containerenv`,
    // ports stay bound, and later starts fail. Report the fault instead; repair
    // must be deliberate/operator-driven.
    match heal_podman_state().await {
        Ok(PodmanHealOutcome::Healthy) => debug!("podman runtime state healthy"),
        Ok(PodmanHealOutcome::Unhealthy) => warn!(
            "podman runtime state is unhealthy at startup — skipping automatic runroot cleanup"
        ),
        Err(e) => warn!(
            "podman self-heal failed (non-fatal, will retry next boot): {:#}",
            e
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PodmanHealOutcome {
    Healthy,
    Unhealthy,
}

async fn heal_podman_state() -> Result<PodmanHealOutcome> {
    if probe_podman_ok().await {
        return Ok(PodmanHealOutcome::Healthy);
    }
    Ok(PodmanHealOutcome::Unhealthy)
}

/// True iff `podman info` returns 0 within 5s. Any timeout, spawn
/// failure, or non-zero exit reads as "wedged" and triggers cleanup.
async fn probe_podman_ok() -> bool {
    use std::time::Duration;
    let probe = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("podman")
            .arg("info")
            .arg("--format=json")
            .output(),
    )
    .await;
    match probe {
        Ok(Ok(out)) => out.status.success(),
        Ok(Err(_)) | Err(_) => false,
    }
}

/// Make sure /var/lib/archipelago/secrets/ stays 0700 owned by archipelago,
/// and every file inside is 0600. The parent dir mode is the load-bearing
/// boundary against host-side reads from other UIDs (rootless container
/// escapes get mapped to UID >= 100000 and can't traverse a 0700/uid=1000
/// directory). The per-file 0600 sweep is defense-in-depth in case some
/// installer wrote a 0644 file.
async fn tighten_secrets_dir() -> Result<u32> {
    let dir = Path::new("/var/lib/archipelago/secrets");
    if !dir.exists() {
        return Ok(0);
    }
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .await
        .with_context(|| format!("chmod 0700 {}", dir.display()))?;

    let mut entries = fs::read_dir(dir)
        .await
        .with_context(|| format!("read_dir {}", dir.display()))?;
    let mut tightened = 0u32;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        if meta.permissions().mode() & 0o777 != 0o600 {
            fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .await
                .with_context(|| format!("chmod 0600 {}", path.display()))?;
            tightened += 1;
        }
    }
    Ok(tightened)
}

async fn run_service_override_repair() -> Result<bool> {
    let override_path = Path::new("/etc/systemd/system/archipelago.service.d/override.conf");
    let Ok(content) = fs::read_to_string(override_path).await else {
        return Ok(false);
    };
    if !content.contains("ARCHIPELAGO_DEV_MODE=true") {
        return Ok(false);
    }

    let only_dev_mode_override = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .all(|line| line == "[Service]" || line == "Environment=ARCHIPELAGO_DEV_MODE=true");
    if !only_dev_mode_override {
        warn!(
            path = %override_path.display(),
            "Archipelago service override contains ARCHIPELAGO_DEV_MODE=true plus other settings; leaving it untouched"
        );
        return Ok(false);
    }

    let path_s = override_path.to_string_lossy().to_string();
    let status = host_sudo(&["rm", "-f", &path_s])
        .await
        .with_context(|| format!("remove {}", override_path.display()))?;
    if !status.success() {
        anyhow::bail!("remove {} exited with {}", override_path.display(), status);
    }
    let _ = host_sudo(&["systemctl", "daemon-reload"]).await;
    Ok(true)
}

async fn run_runtime_assets() -> Result<bool> {
    // The v1.7.50 OTA bridge puts scripts/apps/docker assets inside the
    // frontend tarball because older binaries only know how to apply the
    // backend binary and frontend archive. Once the new backend starts, it
    // promotes that payload into /opt so app installs use the matching specs.
    let runtime_dir = Path::new(RUNTIME_ASSETS_DIR);
    if !runtime_dir.exists() {
        return Ok(false);
    }

    let mut changed = false;
    for (relative, dest) in [
        ("apps", "/opt/archipelago/apps"),
        ("scripts", "/opt/archipelago/scripts"),
        ("docker", "/opt/archipelago/docker"),
    ] {
        let src = runtime_dir.join(relative);
        if src.exists() {
            replace_dir_from_runtime(&src, dest).await?;
            if relative == "scripts" {
                let _ = host_sudo(&[
                    "find", dest, "-type", "f", "-name", "*.sh", "-exec", "chmod", "755", "{}", "+",
                ])
                .await;
                let image_versions = format!("{}/image-versions.sh", dest);
                if Path::new(&image_versions).exists() {
                    let _ =
                        host_sudo(&["cp", &image_versions, "/opt/archipelago/image-versions.sh"])
                            .await;
                }
            }
            changed = true;
        }
    }

    let configs = runtime_dir.join("image-recipe/configs");
    let nginx_src = configs.join("nginx-archipelago.conf");
    if nginx_src.exists() {
        let src_s = nginx_src.to_string_lossy().to_string();
        let status = host_sudo(&[
            "install",
            "-m",
            "644",
            &src_s,
            "/etc/nginx/sites-available/archipelago",
        ])
        .await
        .context("install nginx-archipelago.conf")?;
        if !status.success() {
            anyhow::bail!("install nginx-archipelago.conf exited with {}", status);
        }
        changed = true;
    }

    // archipelago-host-secrets-audit.service rides this same path (phase 10
    // KEY-02 / audit F-03). Nodes already in the field never received 10-03's
    // ISO-build fix — the first-boot script is installed by the installer, not
    // shipped by OTA — so a node that hit the old fail-open path is still on
    // the SSH host key and TLS private key baked into its published ISO, and
    // will never try again. This unit is how such a node reports itself. It is
    // DETECT ONLY (D-06: detect-report-then-apply); rotation is operator-driven
    // via `--apply --yes` and never fires from a unit.
    let mut host_secrets_unit_installed = false;
    for unit in [
        "archipelago-doctor.service",
        "archipelago-doctor.timer",
        "archipelago-host-secrets-audit.service",
    ] {
        let src = configs.join(unit);
        if src.exists() {
            let src_s = src.to_string_lossy().to_string();
            let dest = format!("/etc/systemd/system/{}", unit);
            let status = host_sudo(&["install", "-m", "644", &src_s, &dest])
                .await
                .with_context(|| format!("install {}", unit))?;
            if !status.success() {
                anyhow::bail!("install {} exited with {}", unit, status);
            }
            if unit == "archipelago-host-secrets-audit.service" {
                host_secrets_unit_installed = true;
            }
            changed = true;
        }
    }

    // Packaged radio tools (v1.7.118+): OTA updates only apply the backend
    // binary and frontend tarball, so these PyInstaller binaries ride the
    // runtime payload and get promoted here. Without this, fleet nodes keep
    // a stale archy-reticulum-daemon (which exits on flags it doesn't know —
    // the v1.7.117 --enable-transport rollout killed their mesh sessions)
    // and never receive archy-rnodeconf at all (Flash LoRa: "No such file
    // or directory"). Skipped when byte-identical; a running daemon is
    // unaffected (install replaces the inode) and picks the new binary up
    // on its next spawn.
    for tool in ["archy-reticulum-daemon", "archy-rnodeconf"] {
        let src = runtime_dir.join("radio-tools").join(tool);
        if !src.exists() {
            continue;
        }
        let dest = format!("/usr/local/bin/{}", tool);
        let same = match (fs::read(&src).await, fs::read(&dest).await) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if same {
            continue;
        }
        let src_s = src.to_string_lossy().to_string();
        let status = host_sudo(&["install", "-m", "755", &src_s, &dest])
            .await
            .with_context(|| format!("install {}", tool))?;
        if !status.success() {
            anyhow::bail!("install {} exited with {}", tool, status);
        }
        info!(
            tool,
            "Promoted packaged radio tool from OTA runtime payload"
        );
        changed = true;
    }

    if changed {
        let _ = host_sudo(&["systemctl", "daemon-reload"]).await;
        if host_secrets_unit_installed {
            // `--now` on purpose: the verdict is the whole deliverable, and
            // waiting for the next reboot to learn whether a node is running
            // fleet-shared key material wastes the OTA that just delivered the
            // means to find out. The unit is Type=oneshot, read-only and exits
            // in milliseconds on a healthy node. Best-effort: a node that
            // cannot enable it still boots, and the next OTA retries.
            match host_sudo(&[
                "systemctl",
                "enable",
                "--now",
                "archipelago-host-secrets-audit.service",
            ])
            .await
            {
                Ok(status) if status.success() => {
                    info!("Enabled archipelago-host-secrets-audit.service from OTA runtime payload")
                }
                Ok(status) => tracing::warn!(
                    "enabling archipelago-host-secrets-audit.service exited with {}",
                    status
                ),
                Err(e) => tracing::warn!(
                    "failed to enable archipelago-host-secrets-audit.service: {}",
                    e
                ),
            }
        }
        if nginx_src.exists() {
            match host_sudo(&["nginx", "-t"]).await {
                Ok(status) if status.success() => {
                    let _ = host_sudo(&["systemctl", "reload", "nginx"]).await;
                }
                Ok(status) => {
                    tracing::warn!("nginx config test failed after runtime sync: {}", status);
                }
                Err(e) => {
                    tracing::warn!("failed to test nginx config after runtime sync: {}", e);
                }
            }
        }
    }
    Ok(changed)
}

async fn replace_dir_from_runtime(src: &Path, dest: &str) -> Result<()> {
    let tmp = format!("{}.new.{}", dest, chrono::Utc::now().timestamp_millis());
    let src_dot = path_dot(src);
    let mkdir = host_sudo(&["mkdir", "-p", &tmp])
        .await
        .with_context(|| format!("mkdir {}", tmp))?;
    if !mkdir.success() {
        anyhow::bail!("mkdir {} exited with {}", tmp, mkdir);
    }
    let copy = host_sudo(&["cp", "-a", &src_dot, &tmp])
        .await
        .with_context(|| format!("copy runtime {} -> {}", src.display(), tmp))?;
    if !copy.success() {
        let _ = host_sudo(&["rm", "-rf", &tmp]).await;
        anyhow::bail!("copy runtime {} exited with {}", src.display(), copy);
    }
    let _ = host_sudo(&["mkdir", "-p", dest]).await;
    let cleanup = host_sudo(&[
        "find",
        dest,
        "-mindepth",
        "1",
        "-maxdepth",
        "1",
        "-exec",
        "rm",
        "-rf",
        "{}",
        "+",
    ])
    .await
    .with_context(|| format!("clean {}", dest))?;
    if !cleanup.success() {
        let _ = host_sudo(&["rm", "-rf", &tmp]).await;
        anyhow::bail!("clean {} exited with {}", dest, cleanup);
    }
    let tmp_dot = format!("{}/.", tmp);
    let promote = host_sudo(&["cp", "-a", &tmp_dot, dest])
        .await
        .with_context(|| format!("promote {} -> {}", tmp, dest))?;
    let _ = host_sudo(&["rm", "-rf", &tmp]).await;
    if !promote.success() {
        anyhow::bail!("promote {} exited with {}", dest, promote);
    }
    Ok(())
}

fn path_dot(path: &Path) -> String {
    let mut p = PathBuf::from(path);
    p.push(".");
    p.to_string_lossy().to_string()
}

/// ISO installs before the auto-install.sh path fix copied the app manifests
/// to /etc/archipelago/apps while the backend loads them from
/// /opt/archipelago/apps — so fresh nodes had ZERO disk manifests and only
/// catalog-covered apps could install (netbird "manifests not available",
/// framework node 2026-07-14). Self-heal: when /opt has no manifests and the
/// installer copy exists, populate /opt from /etc. Never overwrites existing
/// /opt manifests (OTA runtime-assets sync owns those afterwards).
async fn run_apps_dir_repair() -> Result<bool> {
    let script = r#"
set -eu
src=/etc/archipelago/apps
dst=/opt/archipelago/apps
[ -d "$src" ] || exit 0
# Only heal when the destination has no manifests at all.
if [ -d "$dst" ] && [ -n "$(ls -A "$dst" 2>/dev/null)" ]; then exit 0; fi
ls "$src"/*/manifest.yml >/dev/null 2>&1 || exit 0
mkdir -p "$dst"
cp -r "$src"/. "$dst"/
exit 2
"#;
    let status = host_sudo(&["sh", "-lc", script])
        .await
        .context("populate /opt/archipelago/apps from installer copy")?;
    match status.code() {
        Some(0) => Ok(false),
        Some(2) => Ok(true),
        _ => {
            warn!("Apps dir repair helper exited with {}", status);
            Ok(false)
        }
    }
}

/// Self-heal Wi-Fi setup on nodes that predate the polkit fix (issue #99).
///
/// Archipelago drives NetworkManager from a system-level systemd service
/// (`User=archipelago`, no logind seat), so the stock NM polkit rule — which
/// only authorizes `subject.local && subject.active` sessions — denies it, and
/// "connect to Wi-Fi" fails with "Insufficient privileges". Fresh ISO installs
/// since 2026-05 ship the rule below, but nodes that reached this build over
/// OTA never got it (OTA replaces the binary + web UI, not host system config).
///
/// Install the scoped rule if it is missing, and best-effort ensure `polkitd`
/// itself is present (without the daemon the rule is inert). Both are wrapped
/// so an offline/locked apt or a missing package can never fail startup — the
/// rule is still written so it takes effect once polkitd arrives (e.g. after an
/// ISO reflash). Idempotent: keyed on the rule's unique `subject.user` marker.
async fn run_polkit_networkmanager_repair() -> Result<bool> {
    let script = r#"
set -u
RULE=/etc/polkit-1/rules.d/49-archipelago-networkmanager.rules
MARKER='subject.user == "archipelago"'
# Rule already installed — nothing to do.
if grep -qF "$MARKER" "$RULE" 2>/dev/null; then
    exit 0
fi
# The rule is inert without the polkit daemon. Older nodes (the ones that hit
# issue #99) shipped without it. Try to install it, but never let apt failure
# (offline node, locked dpkg, package unavailable) abort the heal — the rule is
# written regardless so it activates whenever polkitd lands.
if [ ! -d /usr/share/polkit-1 ] && ! command -v pkaction >/dev/null 2>&1; then
    timeout 240 apt-get install -y --no-install-recommends polkitd >/dev/null 2>&1 \
        || timeout 240 sh -c 'apt-get update >/dev/null 2>&1 && apt-get install -y --no-install-recommends polkitd >/dev/null 2>&1' \
        || true
fi
mkdir -p /etc/polkit-1/rules.d
cat > "$RULE" <<'RULEEOF'
polkit.addRule(function(action, subject) {
    if (subject.user == "archipelago" && action.id.indexOf("org.freedesktop.NetworkManager.") == 0) {
        return polkit.Result.YES;
    }
});
RULEEOF
chmod 644 "$RULE"
# Pick up the new rule. polkitd re-reads rules.d on reload; restart as a
# fallback. Non-fatal if the unit name differs or the daemon is absent.
systemctl reload polkit 2>/dev/null \
    || systemctl restart polkit 2>/dev/null \
    || systemctl restart polkit.service 2>/dev/null \
    || true
exit 2
"#;
    let status = host_sudo(&["sh", "-lc", script])
        .await
        .context("install NetworkManager polkit rule")?;
    match status.code() {
        Some(0) => Ok(false),
        Some(2) => Ok(true),
        _ => {
            warn!("polkit NetworkManager repair helper exited with {}", status);
            Ok(false)
        }
    }
}

/// Repair Tor at boot so a torrc-generation fix actually reaches the nodes it
/// was written for.
///
/// `regenerate_torrc` only ran from the Tor RPC handlers and package install,
/// so a node carrying a bad torrc kept it indefinitely: shipping a fixed binary
/// changed nothing until somebody happened to install an app or toggle a Tor
/// setting. On 2026-08-09 two of the three reachable nodes had Tor completely
/// down from an unbindable `SocksPort <archy-net gateway>` line, one of them
/// unnoticed, while the dashboard reported "connected".
///
/// Non-fatal and conservative: it only restarts Tor when the regenerated torrc
/// differs from the live one, or Tor is not answering on 9050.
/// The privileged Tor helper, embedded so the OTA actually delivers it.
/// scripts/tor-helper.sh previously reached nodes only through ISO builds and
/// manual deploys — the 2026-08-09 helper fix (reset-failed + truthful result)
/// would have shipped to nobody. Same include_str! pattern as the doctor.
const TOR_HELPER_SH: &str = include_str!("../../../scripts/tor-helper.sh");
const TOR_HELPER_PATH: &str = "/opt/archipelago/scripts/tor-helper.sh";

/// Heal socat forwarder units that were generated with the node's LAN IP
/// baked into `bind=`.
///
/// `archy-ha-btc-rpc-proxy.service` (written on-node during the Pine/HA
/// integration) bound socat to the box's DHCP address at generation time —
/// pasta containers reach the host via its LAN address, so that was the
/// address that worked. Move the box to a new network and the address no
/// longer exists: `bind()` fails and the unit restart-loops forever
/// (framework-pt after relocating, 2026-08-15: restart counter 2446, and
/// Home Assistant's bitcoind sensor dead with it).
///
/// The rewrite computes the bind address at every service start instead, so
/// `Restart=always` itself becomes the heal: plug the box into any network
/// and the next restart binds to the new address.
const HA_RPC_PROXY_UNIT_PATH: &str = "/etc/systemd/system/archy-ha-btc-rpc-proxy.service";

/// Parse `TCP-LISTEN:<port>,bind=<ipv4>` + trailing `TCP:<target>` out of a
/// socat ExecStart line. Returns (listen_port, target).
fn parse_socat_static_bind(exec_line: &str) -> Option<(String, String)> {
    let after_listen = exec_line.split("TCP-LISTEN:").nth(1)?;
    let port = after_listen.split(',').next()?.trim();
    if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Only rewrite units pinned to a concrete address; a unit already using
    // a computed bind (or none) needs no heal.
    let bind = after_listen
        .split("bind=")
        .nth(1)?
        .split(',')
        .next()?
        .trim();
    if !bind.chars().all(|c| c.is_ascii_digit() || c == '.') || bind.starts_with("127.") {
        return None;
    }
    let target = exec_line.rsplit(" TCP:").next()?.trim();
    if target.is_empty() || target == exec_line {
        return None;
    }
    Some((port.to_string(), target.to_string()))
}

fn dynamic_bind_execstart(listen_port: &str, target: &str) -> String {
    // `$$` survives systemd's own expansion as a literal `$`, so the command
    // substitution runs in the shell at ExecStart time. If the box has no
    // default route yet, exit non-zero and let Restart=always retry.
    format!(
        "ExecStart=/bin/sh -c 'IP=$$(ip -4 route get 1.1.1.1 | sed -n \"s/.*src \\([0-9.]*\\).*/\\1/p\"); \
         [ -n \"$$IP\" ] || exit 1; \
         exec /usr/bin/socat TCP-LISTEN:{listen_port},bind=$$IP,fork,reuseaddr TCP:{target}'"
    )
}

async fn run_ha_rpc_proxy_bind_repair() -> Result<bool> {
    let unit = match tokio::fs::read_to_string(HA_RPC_PROXY_UNIT_PATH).await {
        Ok(s) => s,
        Err(_) => return Ok(false), // node never grew the forwarder
    };
    let Some(exec_line) = unit
        .lines()
        .find(|l| l.trim_start().starts_with("ExecStart="))
    else {
        return Ok(false);
    };
    let Some((port, target)) = parse_socat_static_bind(exec_line) else {
        return Ok(false); // already dynamic (or not the shape we heal)
    };
    let healed = unit.replace(exec_line, &dynamic_bind_execstart(&port, &target));
    let staged = "/var/lib/archipelago/ha-rpc-proxy.staged";
    if let Some(dir) = Path::new(staged).parent() {
        tokio::fs::create_dir_all(dir).await.ok();
    }
    tokio::fs::write(staged, &healed)
        .await
        .context("stage ha-rpc-proxy unit")?;
    let script = format!(
        "set -eu\ninstall -m 0644 {staged} {dest}\nsystemctl daemon-reload\nsystemctl restart archy-ha-btc-rpc-proxy 2>/dev/null || true\nexit 0\n",
        staged = staged,
        dest = HA_RPC_PROXY_UNIT_PATH
    );
    host_sudo(&["sh", "-lc", &script])
        .await
        .context("install ha-rpc-proxy unit")?;
    Ok(true)
}

/// Re-point `--pull never` quadlets whose image ref no longer matches local
/// storage.
///
/// The catalog signing pass rewrites image refs (bare-IP registry → domain),
/// so a quadlet regenerated with the new ref points at an image the local
/// store only holds under the old name. With `--pull never` the app can
/// never start again on its own — Home Assistant looped 761 restarts on
/// "image not known" (framework-pt, 2026-08-15) while an identical
/// `name:tag` sat in storage under the bare-IP ref. If any local image
/// shares the wanted `name:tag`, retag it; pulling is deliberately NOT
/// attempted here (offline nodes, metered links — the doctor handles pulls).
async fn run_pull_never_image_repair() -> Result<usize> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/archipelago".to_string());
    let quadlet_dir = format!("{home}/.config/containers/systemd");
    let mut wanted: Vec<String> = Vec::new();
    let mut entries = match tokio::fs::read_dir(&quadlet_dir).await {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("container") {
            continue;
        }
        let Ok(text) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        for line in text.lines() {
            if let Some(image) = line.trim().strip_prefix("Image=") {
                let image = image.trim();
                if !image.is_empty() {
                    wanted.push(image.to_string());
                }
            }
        }
    }
    if wanted.is_empty() {
        return Ok(0);
    }
    let local = podman_stdout(&["images", "--format", "{{.Repository}}:{{.Tag}}"]).await;
    let local: Vec<&str> = local
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains("<none>"))
        .collect();
    let mut retagged = 0usize;
    for want in wanted {
        if local.iter().any(|l| *l == want) {
            continue;
        }
        // Same `name:tag`, any registry prefix, is the rename we heal.
        let Some(name_tag) = want.rsplit('/').next() else {
            continue;
        };
        if !name_tag.contains(':') {
            continue;
        }
        let suffix = format!("/{name_tag}");
        let Some(src) = local.iter().find(|l| l.ends_with(&suffix)) else {
            continue;
        };
        let status = tokio::process::Command::new("podman")
            .args(["tag", src, &want])
            .status()
            .await;
        match status {
            Ok(s) if s.success() => {
                info!(from = %src, to = %want, "Retagged image for a --pull never quadlet");
                retagged += 1;
            }
            _ => warn!(from = %src, to = %want, "Image retag failed (non-fatal)"),
        }
    }
    Ok(retagged)
}

async fn podman_stdout(args: &[&str]) -> String {
    match tokio::process::Command::new("podman")
        .args(args)
        .output()
        .await
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        _ => String::new(),
    }
}

/// Keep nginx's per-address HTTPS listeners in step with the addresses the
/// host actually has, and get nginx running if a boot race killed it.
///
/// `scripts/setup-node-ca.sh` writes one `listen <addr>:443 ssl;` per LAN
/// address at the moment it runs (per-address rather than wildcard on
/// purpose: Tailscale holds :443 on the tailnet address). Its idempotency
/// guard then never revisits them. Two ways that takes the WHOLE web UI
/// down — nginx refuses to start if any listen address is missing, so this
/// is not merely an HTTPS outage:
///   1. The node moves networks and the old address no longer exists.
///   2. Even in place, nginx starts before DHCP has assigned the address —
///      and nginx.service ships no `Restart=`, so that single failure is
///      permanent until a human intervenes.
/// Both observed on archi-dev-box, 2026-08-15: nginx dead since boot with
/// `bind() to 192.168.63.240:443 failed (99: Cannot assign requested
/// address)`, and the dashboard simply unreachable.
const NGINX_SITES: [&str; 2] = [
    "/etc/nginx/sites-available/archipelago-http",
    "/etc/nginx/sites-available/archipelago",
];
const NGINX_RESTART_DROPIN: &str =
    "/etc/systemd/system/nginx.service.d/10-archipelago-restart.conf";

/// Global IPv4 addresses on this host, minus Tailscale CGNAT (100.64/10) —
/// the same exclusion `setup-node-ca.sh` applies, for the same reason.
async fn host_lan_addrs() -> Vec<String> {
    let out = tokio::process::Command::new("ip")
        .args(["-o", "-4", "addr", "show", "scope", "global"])
        .output()
        .await;
    let Ok(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().nth(3))
        .filter_map(|cidr| cidr.split('/').next())
        .filter(|a| !is_cgnat(a))
        .map(str::to_string)
        .collect()
}

fn is_cgnat(addr: &str) -> bool {
    let mut parts = addr.split('.');
    let (Some(100), Some(second)) = (
        parts.next().and_then(|p| p.parse::<u8>().ok()),
        parts.next().and_then(|p| p.parse::<u8>().ok()),
    ) else {
        return false;
    };
    (64..=127).contains(&second)
}

/// Rewrite the `listen <ip>:443 ssl;` set for one config's text. Returns the
/// new text when it differs. Lines for absent addresses are dropped and one
/// line per present address is kept, preserving the file's indentation.
fn retarget_https_listeners(text: &str, present: &[String]) -> Option<String> {
    let listen_of = |l: &str| -> Option<String> {
        let t = l.trim();
        let rest = t.strip_prefix("listen ")?.strip_suffix(":443 ssl;")?;
        // Only per-address listeners; `listen 443 ssl ...` has no address.
        rest.split('.').count().eq(&4).then(|| rest.to_string())
    };
    if !text.lines().any(|l| listen_of(l).is_some()) {
        return None; // wildcard-only config; nothing address-pinned to heal
    }
    let stale: Vec<String> = text
        .lines()
        .filter_map(listen_of)
        .filter(|a| !present.contains(a))
        .collect();
    let existing: Vec<String> = text.lines().filter_map(listen_of).collect();
    let missing: Vec<&String> = present.iter().filter(|a| !existing.contains(a)).collect();
    if stale.is_empty() && missing.is_empty() {
        return None;
    }
    let indent = text
        .lines()
        .find(|l| listen_of(l).is_some())
        .map(|l| l[..l.len() - l.trim_start().len()].to_string())
        .unwrap_or_else(|| "    ".to_string());
    let mut out: Vec<String> = Vec::new();
    let mut wrote_block = false;
    for line in text.lines() {
        match listen_of(line) {
            Some(_) if !wrote_block => {
                wrote_block = true;
                for a in present {
                    out.push(format!("{indent}listen {a}:443 ssl;"));
                }
            }
            Some(_) => {} // subsequent old listen lines are replaced by the block
            None => out.push(line.to_string()),
        }
    }
    Some(out.join("\n"))
}

async fn run_nginx_listener_repair() -> Result<bool> {
    let present = host_lan_addrs().await;
    if present.is_empty() {
        return Ok(false); // no network yet; a later boot pass will do it
    }
    let mut changed = false;
    for site in NGINX_SITES {
        let Ok(text) = tokio::fs::read_to_string(site).await else {
            continue;
        };
        let Some(healed) = retarget_https_listeners(&text, &present) else {
            continue;
        };
        let staged = "/var/lib/archipelago/nginx-listeners.staged";
        if let Some(dir) = Path::new(staged).parent() {
            tokio::fs::create_dir_all(dir).await.ok();
        }
        tokio::fs::write(staged, &healed)
            .await
            .context("stage nginx listeners")?;
        // Install behind `nginx -t`, and roll back if the test fails — a bad
        // config here would take the dashboard down, which is the very
        // failure this repair exists to prevent.
        let script = format!(
            "set -eu\ncp {site} {site}.bak-listeners\ninstall -m 0644 {staged} {site}\n\
             if ! nginx -t 2>/dev/null; then cp {site}.bak-listeners {site}; exit 3; fi\nexit 0\n"
        );
        let status = host_sudo(&["sh", "-lc", &script]).await?;
        match status.code() {
            Some(0) => changed = true,
            Some(3) => warn!(
                site,
                "nginx listener repair failed its config test — rolled back"
            ),
            _ => warn!(site, "nginx listener repair helper failed"),
        }
    }
    // Whether or not the config changed: if nginx is down (the boot race, or
    // it died on an address that has since arrived), start it. And give it a
    // restart policy so the race stops being fatal in the first place.
    let script = format!(
        "set -eu\nmkdir -p $(dirname {dropin})\n\
         cat > {dropin} <<'EOF'\n[Service]\nRestart=on-failure\nRestartSec=5\n\
         [Unit]\nStartLimitIntervalSec=300\nStartLimitBurst=10\nEOF\n\
         systemctl daemon-reload\n\
         if ! systemctl is-active --quiet nginx; then systemctl reset-failed nginx 2>/dev/null || true; systemctl start nginx 2>/dev/null || true; \
         elif [ \"${{RELOAD:-1}}\" = 1 ]; then systemctl reload nginx 2>/dev/null || true; fi\nexit 0\n",
        dropin = NGINX_RESTART_DROPIN
    );
    host_sudo(&["sh", "-lc", &script])
        .await
        .context("nginx restart policy + start")?;
    Ok(changed)
}

/// The console welcome banner, embedded so the OTA can fix it on deployed
/// nodes. `/etc/profile.d/archipelago.sh` is baked by the ISO installer and
/// no OTA path touched it, so every node kept whatever its ISO generation
/// shipped — including banners that print the node's own WireGuard address
/// (10.44.0.1, present on EVERY node) as the "web ui", which is unreachable
/// off-tunnel and actively misleading after a move to a new network
/// (framework-pt, 2026-08-15). Canonical copy: scripts/welcome-banner.sh;
/// the ISO builder inlines the same content for fresh installs.
const WELCOME_BANNER_SH: &str = include_str!("../../../scripts/welcome-banner.sh");
const WELCOME_BANNER_PATH: &str = "/etc/profile.d/archipelago.sh";

async fn run_welcome_banner_sync() -> Result<bool> {
    let current = tokio::fs::read_to_string(WELCOME_BANNER_PATH)
        .await
        .unwrap_or_default();
    // Only refresh a banner the installer put there: a dev machine running
    // the backend from a checkout has no business growing one in /etc.
    if current.is_empty() || current == WELCOME_BANNER_SH {
        return Ok(false);
    }
    let staged = "/var/lib/archipelago/welcome-banner.staged";
    if let Some(dir) = Path::new(staged).parent() {
        tokio::fs::create_dir_all(dir).await.ok();
    }
    tokio::fs::write(staged, WELCOME_BANNER_SH)
        .await
        .context("stage welcome banner")?;
    let script = format!(
        "set -eu\ninstall -m 0755 {staged} {dest}\nexit 0\n",
        staged = staged,
        dest = WELCOME_BANNER_PATH
    );
    host_sudo(&["sh", "-lc", &script])
        .await
        .context("install welcome banner")?;
    Ok(true)
}

async fn run_tor_helper_sync() -> Result<bool> {
    let current = tokio::fs::read_to_string(TOR_HELPER_PATH)
        .await
        .unwrap_or_default();
    if current == TOR_HELPER_SH {
        return Ok(false);
    }
    let staged = "/var/lib/archipelago/tor-config/tor-helper.staged";
    if let Some(dir) = Path::new(staged).parent() {
        tokio::fs::create_dir_all(dir).await.ok();
    }
    tokio::fs::write(staged, TOR_HELPER_SH)
        .await
        .context("stage tor-helper.sh")?;
    let script = format!(
        "set -eu\ninstall -m 0755 {staged} {dest}\nexit 0\n",
        staged = staged,
        dest = TOR_HELPER_PATH
    );
    host_sudo(&["sh", "-lc", &script])
        .await
        .context("install tor-helper.sh")?;
    Ok(true)
}

/// Existing nodes' nginx configs never receive repo snippet fixes — the OTA
/// updates the binary and web assets, not /etc/nginx. The mempool UI is
/// websocket-driven, and every fleet node's /app/mempool/ proxy block strips
/// the Upgrade handshake, so the page loads and never connects (three-layer
/// outage, 2026-08-09). Idempotently add the two headers to any mempool block
/// missing them, in every nginx file that has one, then reload once.
async fn run_nginx_mempool_ws_repair() -> Result<bool> {
    let script = r#"
set -eu
changed=0
for f in /etc/nginx/sites-available/archipelago-http \
         /etc/nginx/sites-available/archipelago \
         /etc/nginx/snippets/archipelago-https-app-proxies.conf; do
  [ -f "$f" ] || continue
  grep -q 'location /app/mempool/' "$f" || continue
  python3 - "$f" <<'PYEOF'
import re, sys
p = sys.argv[1]
src = open(p).read()
def fix(m):
    b = m.group(0)
    if 'Upgrade $http_upgrade' in b:
        return b
    return b.replace('proxy_http_version 1.1;',
        'proxy_http_version 1.1;\n        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection "upgrade";', 1)
new = re.sub(r'location /app/mempool/ \{[^}]*\}', fix, src, flags=re.S)
if new != src:
    open(p, 'w').write(new)
    sys.exit(3)
PYEOF
  rc=$?
  [ "$rc" = 3 ] && changed=1
  [ "$rc" = 0 ] || [ "$rc" = 3 ] || exit "$rc"
done
if [ "$changed" = 1 ]; then
  nginx -t >/dev/null 2>&1 && systemctl reload nginx || true
  exit 3
fi
exit 0
"#;
    let status = host_sudo(&["sh", "-lc", script])
        .await
        .context("nginx mempool ws repair")?;
    Ok(status.code() == Some(3))
}

async fn run_tor_torrc_repair() -> Result<bool> {
    // Same location the RPC handlers use (Config::data_dir); bootstrap runs
    // before the server owns a Config, and this path is fixed on real installs.
    let data_dir = Path::new("/var/lib/archipelago");
    if !data_dir.exists() {
        debug!("No {} — skipping Tor boot heal", data_dir.display());
        return Ok(false);
    }
    if !Path::new("/etc/tor/torrc").exists() {
        debug!("No /etc/tor/torrc — Tor not installed here, skipping boot heal");
        return Ok(false);
    }
    crate::api::rpc::tor::heal_on_boot(data_dir).await
}

async fn run_bitcoin_rpc_repair() -> Result<bool> {
    // bitcoind is launched with -conf=/tmp/rpc.conf and never reads a
    // datadir bitcoin.conf (apps/bitcoin-core & bitcoin-knots manifest.yml,
    // commit a597c1d9 — bind/port live only on the container command line).
    // A leftover file from an older install makes Bitcoin Core's own
    // datadir-conflict safety check refuse to start on every subsequent
    // start. Remove it instead of "repairing" it into existence — this
    // previously wrote server=/rpcbind=/rpcallowip=/listen= into the file,
    // which is exactly what caused the conflict.
    let script = r#"
set -eu
conf=/var/lib/archipelago/bitcoin/bitcoin.conf
[ -f "$conf" ] || exit 0
mv "$conf" "$conf.disabled-$(date +%s)"
exit 2
"#;
    let status = host_sudo(&["sh", "-lc", script])
        .await
        .context("remove stale bitcoin.conf RPC bind settings")?;
    match status.code() {
        Some(0) => Ok(false),
        // Do not restart Bitcoin from bootstrap. During IBD, an automatic
        // restart can cost hours of progress. Removing the stale file is
        // only a fallback for future starts; current containers keep their
        // command-line RPC args regardless.
        Some(2) => Ok(true),
        _ => {
            warn!("Bitcoin RPC repair helper exited with {}", status);
            Ok(false)
        }
    }
}

/// Install the journald log-volume policy drop-in (JOURNALD_DROPIN) so nodes
/// deployed before the ISO shipped it get the size cap + rate limit via OTA.
/// Idempotent; restarts journald only when the file actually changed (safe:
/// the sockets are held by pid1, so at most a few messages queue briefly).
async fn run_journald_dropin() -> Result<bool> {
    // Same dev-box guards as the doctor bootstrap: never touch /etc on
    // contributors' laptops (symlinked or absent /home/archipelago/archy).
    let home_archy = Path::new("/home/archipelago/archy");
    if fs::symlink_metadata(home_archy)
        .await
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        debug!("/home/archipelago/archy is a symlink — skipping journald bootstrap (dev box)");
        return Ok(false);
    }
    if fs::metadata(home_archy).await.is_err() {
        debug!("/home/archipelago/archy missing — skipping journald bootstrap");
        return Ok(false);
    }

    let dropin_dir = "/etc/systemd/journald.conf.d";
    let status = host_sudo(&["mkdir", "-p", dropin_dir])
        .await
        .with_context(|| format!("mkdir {}", dropin_dir))?;
    if !status.success() {
        anyhow::bail!("mkdir {} exited with {}", dropin_dir, status);
    }

    let changed = write_root_if_needed(JOURNALD_DROPIN_PATH, JOURNALD_DROPIN).await?;
    if changed {
        if let Err(e) = host_sudo(&["systemctl", "restart", "systemd-journald"]).await {
            warn!("journald restart after drop-in update failed: {:#}", e);
        }
    }
    Ok(changed)
}

async fn run() -> Result<bool> {
    // Dev-box guard: on contributors' laptops `/home/archipelago/archy` is
    // typically a symlink into the git checkout, and writing through it
    // would clobber the working tree with whatever the binary happens to
    // have been compiled from. Production ISO installs materialize a real
    // directory.
    let home_archy = Path::new("/home/archipelago/archy");
    if fs::symlink_metadata(home_archy)
        .await
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        debug!("/home/archipelago/archy is a symlink — skipping doctor bootstrap (dev box)");
        return Ok(false);
    }

    // Skip entirely on machines without the canonical scripts directory —
    // writing orphan files there just causes confusion.
    let scripts_dir = Path::new(DOCTOR_SH_PATH)
        .parent()
        .context("doctor script path has no parent")?;
    if !scripts_dir.exists() {
        debug!(
            "Scripts dir {} missing — skipping doctor bootstrap",
            scripts_dir.display()
        );
        return Ok(false);
    }

    let mut changed = false;

    // 1. Script — lives in the canonical OTA runtime scripts directory.
    if needs_write(DOCTOR_SH_PATH, DOCTOR_SH).await {
        fs::write(DOCTOR_SH_PATH, DOCTOR_SH)
            .await
            .with_context(|| format!("write {}", DOCTOR_SH_PATH))?;
        let _ = tokio::process::Command::new("chmod")
            .args(["+x", DOCTOR_SH_PATH])
            .status()
            .await;
        info!("Updated {}", DOCTOR_SH_PATH);
        changed = true;
    }

    // 2. Systemd unit files — /etc is restricted; route through host_sudo.
    let service_changed = write_root_if_needed(DOCTOR_SERVICE_PATH, DOCTOR_SERVICE).await?;
    let timer_changed = write_root_if_needed(DOCTOR_TIMER_PATH, DOCTOR_TIMER).await?;
    changed = changed || service_changed || timer_changed;

    // 3. Reload if units changed. Do not enable/start the timer here: lifecycle
    // qualification and explicit app operations need deterministic Podman
    // ownership, and the doctor can race those flows. Operators can enable it
    // separately when they want periodic host repair.
    if service_changed || timer_changed {
        if let Err(e) = host_sudo(&["systemctl", "daemon-reload"]).await {
            warn!("daemon-reload failed: {:#}", e);
        }
    }

    Ok(changed)
}

async fn needs_write(path: &str, expected: &str) -> bool {
    match fs::read_to_string(path).await {
        Ok(current) => current != expected,
        Err(_) => true,
    }
}

/// Write content to a root-owned path via `sudo mv` of a user-owned tmp file.
/// Returns true if a write happened.
async fn write_root_if_needed(path: &str, content: &str) -> Result<bool> {
    if !needs_write(path, content).await {
        return Ok(false);
    }
    let tmp = format!(
        "/tmp/archipelago-bootstrap-{}-{}.tmp",
        std::process::id(),
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unit")
    );
    fs::write(&tmp, content)
        .await
        .with_context(|| format!("write tmp {}", tmp))?;
    let status = host_sudo(&["mv", &tmp, path])
        .await
        .with_context(|| format!("sudo mv {} -> {}", tmp, path))?;
    if !status.success() {
        let _ = fs::remove_file(&tmp).await;
        anyhow::bail!("sudo mv to {} exited with {}", path, status);
    }
    info!("Updated {}", path);
    Ok(true)
}

const ARCHIPELAGO_SERVICE_PATH: &str = "/etc/systemd/system/archipelago.service";
const MOUNT_REQUIRE_LINE: &str = "RequiresMountsFor=/var/lib/archipelago";

/// B17 self-heal: ensure the installed archipelago.service waits for the data
/// volume to mount before it starts. On production nodes `/var/lib/archipelago`
/// (the app data dir AND podman's graphroot) is a separate device-mapper volume;
/// without a mount dependency the service can start before `var-lib-archipelago.mount`,
/// write to the bare mountpoint on rootfs, fail every podman call, exit, and be
/// restarted every 5s until the volume mounts (~5 min of "[FAILED] Failed to start"
/// on cold boots). Fresh ISOs already ship the directive; this heals already-deployed
/// nodes. The change is boot-ordering only — it takes effect on the NEXT reboot, so we
/// never restart the running service here. Idempotent; no-op if the unit is absent
/// (dev runs) or already patched. Harmless when the data dir is on rootfs (systemd maps
/// the requirement to the always-mounted root).
pub async fn ensure_archipelago_mount_ordering() {
    let current = match fs::read_to_string(ARCHIPELAGO_SERVICE_PATH).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(
                "mount-ordering self-heal: {} not readable ({}) — skipping",
                ARCHIPELAGO_SERVICE_PATH,
                e
            );
            return;
        }
    };
    if current.contains(MOUNT_REQUIRE_LINE) {
        return; // already healed
    }
    // Insert the directive into the [Unit] section, immediately before [Service].
    let Some(idx) = current.find("\n[Service]") else {
        tracing::warn!(
            "mount-ordering self-heal: no [Service] section in {} — skipping",
            ARCHIPELAGO_SERVICE_PATH
        );
        return;
    };
    let mut patched = String::with_capacity(current.len() + MOUNT_REQUIRE_LINE.len() + 96);
    patched.push_str(&current[..idx]);
    patched.push_str("\n# B17: start only after the data volume (+ podman graphroot) is mounted\n");
    patched.push_str(MOUNT_REQUIRE_LINE);
    patched.push_str(&current[idx..]);
    match write_root_if_needed(ARCHIPELAGO_SERVICE_PATH, &patched).await {
        Ok(true) => {
            info!(
                "B17: added '{}' to archipelago.service (effective next reboot)",
                MOUNT_REQUIRE_LINE
            );
            if let Err(e) = host_sudo(&["systemctl", "daemon-reload"]).await {
                tracing::warn!("B17 self-heal: daemon-reload failed: {:#}", e);
            }
        }
        Ok(false) => {}
        Err(e) => tracing::warn!("B17 mount-ordering self-heal failed: {:#}", e),
    }
}

/// #36 self-heal: keep the kiosk unit + launcher current on already-deployed
/// nodes so the CPU/memory cap (a runaway chromium was saturating the node and
/// starving the backend) and the GPU-vs-headless flag selection arrive via OTA.
/// No-op on nodes without the kiosk installed; only restarts the kiosk if it's
/// actually running (so it never re-enables an operator-disabled kiosk).
pub async fn ensure_kiosk_hardened() {
    if fs::metadata(KIOSK_SERVICE_PATH).await.is_err() {
        return; // kiosk not installed on this node
    }
    let svc_changed = write_root_if_needed(KIOSK_SERVICE_PATH, KIOSK_SERVICE)
        .await
        .unwrap_or(false);
    let launcher_changed = write_root_if_needed(KIOSK_LAUNCHER_PATH, KIOSK_LAUNCHER)
        .await
        .unwrap_or(false);
    if launcher_changed {
        let _ = host_sudo(&["chmod", "+x", KIOSK_LAUNCHER_PATH]).await;
    }
    if svc_changed || launcher_changed {
        if let Err(e) = host_sudo(&["systemctl", "daemon-reload"]).await {
            warn!("kiosk hardening: daemon-reload failed: {:#}", e);
        }
        // try-restart only restarts a currently-active unit — leaves a stopped/
        // disabled kiosk alone.
        let _ = host_sudo(&["systemctl", "try-restart", "archipelago-kiosk.service"]).await;
        info!("kiosk: applied resource cap + GPU-flag hardening (#36)");
    }
}

/// HDMI-audio self-heal for kiosk nodes: install the PipeWire stack (older
/// ISOs shipped none), put the archipelago user in `audio` (PipeWire runs
/// under the lingering user manager — no logind seat, so no udev ACLs on
/// /dev/snd), and keep the audio-router daemon (HDMI routing + ELD boot-race
/// nudge) installed and current. No-op on nodes without the kiosk — audio
/// only matters where media plays on an attached display.
pub async fn ensure_audio_stack() {
    if fs::metadata(KIOSK_SERVICE_PATH).await.is_err() {
        return; // no kiosk → no display audio to route
    }

    // Package install runs via systemd-run (host_sudo), outside the service
    // sandbox — /usr and the dpkg database are read-only in our namespace.
    if fs::metadata("/usr/bin/pactl").await.is_err() {
        info!("audio: PipeWire stack missing — installing packages");
        let _ = host_sudo(&["apt-get", "update", "-qq"]).await;
        match host_sudo(&[
            "apt-get",
            "install",
            "-y",
            "-qq",
            "--no-install-recommends",
            "pipewire",
            "pipewire-pulse",
            "pipewire-alsa",
            "wireplumber",
            "alsa-utils",
        ])
        .await
        {
            Ok(s) if s.success() => info!("audio: PipeWire stack installed"),
            Ok(s) => {
                warn!(
                    "audio: package install exited with {} — will retry next start",
                    s
                );
                return;
            }
            Err(e) => {
                warn!(
                    "audio: package install failed: {:#} — will retry next start",
                    e
                );
                return;
            }
        }
    }

    let _ = host_sudo(&["usermod", "-aG", "audio", "archipelago"]).await;

    let unit_was_missing = fs::metadata(AUDIO_SERVICE_PATH).await.is_err();
    let script_changed = write_root_if_needed(AUDIO_ROUTER_PATH, AUDIO_ROUTER)
        .await
        .unwrap_or(false);
    if script_changed {
        let _ = host_sudo(&["chmod", "+x", AUDIO_ROUTER_PATH]).await;
    }
    let unit_changed = write_root_if_needed(AUDIO_SERVICE_PATH, AUDIO_SERVICE)
        .await
        .unwrap_or(false);

    if script_changed || unit_changed {
        if let Err(e) = host_sudo(&["systemctl", "daemon-reload"]).await {
            warn!("audio: daemon-reload failed: {:#}", e);
        }
    }
    if unit_was_missing {
        // First install on this node — bring it up now and on every boot.
        let _ = host_sudo(&[
            "systemctl",
            "enable",
            "--now",
            "archipelago-audio-router.service",
        ])
        .await;
        info!("audio: router installed and enabled (HDMI routing + ELD heal)");
    } else if script_changed || unit_changed {
        // Content update: restart only if it's running — never re-enable a
        // unit an operator deliberately disabled.
        let _ = host_sudo(&[
            "systemctl",
            "try-restart",
            "archipelago-audio-router.service",
        ])
        .await;
        info!("audio: router updated");
    }
}

/// Gamepad→keyboard bridge self-heal for kiosk nodes: keeps the evdev→uinput
/// daemon (controller works inside every app iframe on the TV) installed and
/// current. Same gating as audio: no kiosk → no display input to bridge.
pub async fn ensure_gamepad_keys() {
    if fs::metadata(KIOSK_SERVICE_PATH).await.is_err() {
        return;
    }
    let unit_was_missing = fs::metadata(GAMEPAD_SERVICE_PATH).await.is_err();
    let script_changed = write_root_if_needed(GAMEPAD_KEYS_PATH, GAMEPAD_KEYS)
        .await
        .unwrap_or(false);
    if script_changed {
        let _ = host_sudo(&["chmod", "+x", GAMEPAD_KEYS_PATH]).await;
    }
    let unit_changed = write_root_if_needed(GAMEPAD_SERVICE_PATH, GAMEPAD_SERVICE)
        .await
        .unwrap_or(false);
    if script_changed || unit_changed {
        if let Err(e) = host_sudo(&["systemctl", "daemon-reload"]).await {
            warn!("gamepad bridge: daemon-reload failed: {:#}", e);
        }
    }
    if unit_was_missing {
        let _ = host_sudo(&[
            "systemctl",
            "enable",
            "--now",
            "archipelago-gamepad-keys.service",
        ])
        .await;
        info!("gamepad: bridge installed and enabled (TV controller input)");
    } else if script_changed || unit_changed {
        let _ = host_sudo(&[
            "systemctl",
            "try-restart",
            "archipelago-gamepad-keys.service",
        ])
        .await;
        info!("gamepad: bridge updated");
    }
}

/// Patch the nginx site config to add missing backend proxy blocks. Older ISO
/// configs shipped individual per-endpoint `location` blocks, so missing
/// endpoints silently fell through to the SPA `index.html` and the frontend
/// got HTML instead of JSON.
///
/// Validates via `nginx -t` before reloading. On failure the patch is
/// rolled back from a backup written just before the write.
async fn run_nginx() -> Result<bool> {
    // Skip on dev symlinks — we don't want to touch `/etc/nginx` on laptops.
    let home_archy = Path::new("/home/archipelago/archy");
    if fs::symlink_metadata(home_archy)
        .await
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Ok(false);
    }

    let mut changed = false;
    let mut patched_paths = Vec::<PathBuf>::new();
    for path in [
        NGINX_CONF_PATH,
        NGINX_ENABLED_CONF_PATH,
        NGINX_HTTPS_SNIPPET_PATH,
    ] {
        let candidate = Path::new(path);
        if !candidate.exists() {
            debug!("{} missing — skipping nginx bootstrap", path);
            continue;
        }
        let canonical = fs::canonicalize(candidate)
            .await
            .unwrap_or_else(|_| candidate.to_path_buf());
        if patched_paths.iter().any(|p| p == &canonical) {
            continue;
        }
        patched_paths.push(canonical);
        changed |= patch_nginx_conf(path).await?;
    }
    Ok(changed)
}

/// Reflective CORS add_headers that older configs placed inside the
/// `/lnd-connect-info` location. The backend now sets a validated
/// `Access-Control-Allow-Origin` for that endpoint (api/handler/proxy.rs), so
/// leaving these in nginx emits a DUPLICATE header ("contains multiple values
/// … but only one is allowed") and the LND wallet UI's cross-origin fetch is
/// rejected. Stripped during nginx bootstrap so the backend solely owns CORS.
const NGINX_LND_DUP_CORS: &str = "        add_header Access-Control-Allow-Origin $http_origin always;\n        add_header Access-Control-Allow-Credentials \"true\" always;\n";

/// S4 follow-up (2026-08-07): pre-fix nodes proxy /aiui/api/web-search
/// STRAIGHT to SearXNG (127.0.0.1:8888) with no session check — anyone on the
/// LAN can run searches attributed to the node's IP. The canonical conf routes
/// it through the session-gated daemon (5678) and forwards the Cookie. The
/// stale target string is unique to that block, so a plain replace is safe.
/// Pure and testable; `None` when the stale shape is absent.
fn heal_stale_web_search_block(content: &str) -> Option<String> {
    if !content.contains("proxy_pass http://127.0.0.1:8888/search;") {
        return None;
    }
    Some(content.replace(
        "proxy_pass http://127.0.0.1:8888/search;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;",
        "proxy_pass http://127.0.0.1:5678;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header Cookie $http_cookie;",
    ))
}

fn heal_missing_source_proxy(content: &str) -> Option<String> {
    if content.contains("location /app/archipelago-source/") {
        return None;
    }
    let indented_anchor = "    location /app/gitea/ {";
    if content.contains(indented_anchor) {
        return Some(content.replace(
            indented_anchor,
            &format!("{}{}", NGINX_SOURCE_PROXY_BLOCK, indented_anchor),
        ));
    }
    let snippet_anchor = "location /app/gitea/ {";
    content.contains(snippet_anchor).then(|| {
        content.replace(
            snippet_anchor,
            &format!("{}{}", NGINX_SOURCE_PROXY_BLOCK_SNIPPET, snippet_anchor),
        )
    })
}

/// Older same-origin GitWorkshop blocks stripped the app mount but did not
/// tell the app gate what was stripped. Its challenge therefore posted to
/// `/__archipelago-gate/login` on the dashboard and nginx returned 405. Add
/// the mount header to every canonical source block (HTTP and HTTPS snippet).
fn heal_source_forwarded_prefix(content: &str) -> Option<String> {
    if !content.contains("proxy_pass http://127.0.0.2:8337/;") {
        return None;
    }
    let mut healed = content.to_owned();
    for indent in ["        ", "    "] {
        let old = format!(
            "proxy_pass http://127.0.0.2:8337/;\n{indent}proxy_http_version 1.1;\n{indent}proxy_set_header Host $http_host;\n{indent}proxy_set_header Cookie $http_cookie;\n{indent}proxy_set_header X-Real-IP $remote_addr;\n{indent}proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n{indent}proxy_set_header X-Forwarded-Proto $scheme;\n{indent}proxy_hide_header X-Frame-Options;"
        );
        let new = old.replace(
            &format!("\n{indent}proxy_hide_header X-Frame-Options;"),
            &format!(
                "\n{indent}proxy_set_header X-Forwarded-Prefix /app/archipelago-source;\n{indent}proxy_hide_header X-Frame-Options;"
            ),
        );
        healed = healed.replace(&old, &new);
    }
    (healed != content).then_some(healed)
}

fn heal_missing_nostr_signer(content: &str) -> Option<String> {
    if content.contains("location = /nostr-signer") {
        return None;
    }
    // The anchor occurs once in each complete HTTP/HTTPS dashboard server and
    // does not occur in the separate app-proxy snippet.
    let anchor = "    location /aiui/ {";
    content
        .contains(anchor)
        .then(|| content.replace(anchor, &format!("{}{}", NGINX_NOSTR_SIGNER_BLOCK, anchor)))
}

async fn patch_nginx_conf(path: &str) -> Result<bool> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("read {}", path))?;
    // Each "missing" flag is gated on the splice anchor actually being present,
    // so an included snippet that legitimately has none of these endpoints (the
    // HTTPS app-proxy snippet) neither tries to patch them nor logs warn-skips on
    // every boot — it falls through to the fedimint heal alone.
    let has_lnd_anchor = content.contains("    location /lnd-connect-info {")
        || content.contains("    location /electrs-status {");
    let missing_app_catalog = content
        .contains("    # DWN endpoints — peer access over Tor (no auth)")
        && !content.contains("location /api/app-catalog");
    let missing_bitcoin_status = content.contains("    location /electrs-status {")
        && !content.contains("location /bitcoin-status");
    let missing_lnd_proxy = has_lnd_anchor && !content.contains("location /proxy/lnd/");
    let missing_peer_content = has_lnd_anchor && !content.contains("location /api/peer-content");
    let missing_pine_status = has_lnd_anchor && !content.contains("location /api/pine/status");
    let has_lnd_dup_cors = content.contains(NGINX_LND_DUP_CORS);
    // B13: fedimint block present but lacking the asset-rewrite sub_filters.
    let needs_fedimint_css = content.contains("location /app/fedimint/")
        && !content.contains("'href=\"/' 'href=\"/app/fedimint/'");
    // Companion mesh access: phones reach this node over FIPS at its fips0
    // ULA (http://[fdxx:…]). Configs shipped before 2026-07-23 listened on
    // IPv4 only, so the ULA could never connect — nothing answered [::]:80.
    let missing_v6_http =
        content.contains("listen 80 default_server;") && !content.contains("listen [::]:80");
    let missing_v6_https =
        content.contains("listen 443 ssl default_server;") && !content.contains("listen [::]:443");
    let stale_web_search = heal_stale_web_search_block(&content).is_some();
    let missing_source_proxy = heal_missing_source_proxy(&content).is_some();
    let missing_source_prefix = heal_source_forwarded_prefix(&content).is_some();
    let missing_nostr_signer = heal_missing_nostr_signer(&content).is_some();
    if !missing_app_catalog
        && !missing_bitcoin_status
        && !missing_lnd_proxy
        && !missing_peer_content
        && !missing_pine_status
        && !has_lnd_dup_cors
        && !needs_fedimint_css
        && !missing_v6_http
        && !missing_v6_https
        && !stale_web_search
        && !missing_source_proxy
        && !missing_source_prefix
        && !missing_nostr_signer
    {
        return Ok(false);
    }

    let mut patched = content.clone();

    if let Some(p) = heal_stale_web_search_block(&patched) {
        patched = p;
    }
    if let Some(p) = heal_missing_source_proxy(&patched) {
        patched = p;
    }
    if let Some(p) = heal_source_forwarded_prefix(&patched) {
        patched = p;
    }
    if let Some(p) = heal_missing_nostr_signer(&patched) {
        patched = p;
    }

    if missing_v6_http {
        patched = patched.replace(
            "listen 80 default_server;",
            "listen 80 default_server;\n    listen [::]:80 default_server;",
        );
    }
    if missing_v6_https {
        patched = patched.replace(
            "listen 443 ssl default_server;",
            "listen 443 ssl default_server;\n    listen [::]:443 ssl default_server;",
        );
    }

    if has_lnd_dup_cors {
        // Drop the redundant nginx-side CORS headers so the backend's single
        // validated Access-Control-Allow-Origin is the only one returned.
        patched = patched.replace(NGINX_LND_DUP_CORS, "");
    }

    if needs_fedimint_css {
        // Style A (main conf): the block already injects nostr-provider, so swap
        // its single-sub_filter tail for the full asset-rewrite set. No-op if the
        // node's fedimint block doesn't match OLD.
        patched = patched.replace(NGINX_FEDIMINT_OLD, NGINX_FEDIMINT_NEW);
        // Style B (HTTPS app-proxy snippet): the block has no sub_filter to swap,
        // so insert the reroot set after the unique :8175 proxy_pass. Guarded on
        // the marker so it can never double-apply after Style A already healed.
        if !patched.contains("'href=\"/' 'href=\"/app/fedimint/'") {
            patched = patched.replace(NGINX_FEDIMINT_SNIPPET_ANCHOR, NGINX_FEDIMINT_SNIPPET_INSERT);
        }
    }

    if missing_lnd_proxy {
        // Prefer the `/lnd-connect-info` anchor (present since 2026-03-17); fall
        // back to `/electrs-status` (since 2026-03-08) for even older configs.
        // Both appear once per archipelago server block, so the block is added
        // to every server block that proxies to the backend.
        let anchor = if patched.contains("    location /lnd-connect-info {") {
            "    location /lnd-connect-info {"
        } else {
            "    location /electrs-status {"
        };
        if !patched.contains(anchor) {
            warn!("nginx conf missing lnd-connect-info/electrs-status anchor — skipping /proxy/lnd patch");
        } else {
            let replacement = format!("{}{}", NGINX_LND_PROXY_BLOCK, anchor);
            patched = patched.replace(anchor, &replacement);
        }
    }

    if missing_pine_status {
        // Same anchoring as the LND proxy: prepend to every server block that
        // proxies to the backend.
        let anchor = if patched.contains("    location /lnd-connect-info {") {
            "    location /lnd-connect-info {"
        } else {
            "    location /electrs-status {"
        };
        if patched.contains(anchor) {
            let replacement = format!("{}{}", NGINX_PINE_STATUS_BLOCK, anchor);
            patched = patched.replace(anchor, &replacement);
        } else {
            warn!("nginx conf missing anchor — skipping /api/pine/status patch");
        }
    }

    if missing_peer_content {
        // Same anchoring as the LND proxy: prepend the block to every server
        // block so /api/peer-content/* reaches the backend instead of the SPA.
        let anchor = if patched.contains("    location /lnd-connect-info {") {
            "    location /lnd-connect-info {"
        } else {
            "    location /electrs-status {"
        };
        if patched.contains(anchor) {
            let replacement = format!("{}{}", NGINX_PEER_CONTENT_BLOCK, anchor);
            patched = patched.replace(anchor, &replacement);
        } else {
            warn!("nginx conf missing anchor — skipping /api/peer-content patch");
        }
    }

    if missing_bitcoin_status {
        let anchor = "    location /electrs-status {";
        if !patched.contains(anchor) {
            warn!("nginx conf missing electrs-status anchor — skipping /bitcoin-status patch");
        } else {
            let replacement = format!("{}{}", NGINX_BITCOIN_STATUS_BLOCK, anchor);
            patched = patched.replace(anchor, &replacement);
        }
    }

    if missing_app_catalog {
        // The DWN comment sits at the same indent right after the `/api/blob`
        // block in both server blocks — a stable anchor that existed on every
        // ISO shipped to date. If it's absent (config got heavily customized),
        // skip rather than guess where to splice.
        let anchor = "    # DWN endpoints — peer access over Tor (no auth)";
        if !patched.contains(anchor) {
            warn!("nginx conf missing DWN anchor — skipping /api/app-catalog patch");
        } else {
            let replacement = format!("{}{}", NGINX_APP_CATALOG_BLOCK, anchor);
            patched = patched.replace(anchor, &replacement);
        }
    }

    if patched == content {
        return Ok(false);
    }

    // Write patched config via a user-owned tmp + sudo mv, after stashing
    // a backup outside nginx include dirs so validation cannot load it too.
    let pid = std::process::id();
    let tmp = format!("/tmp/archipelago-nginx-{}.conf", pid);
    fs::write(&tmp, &patched)
        .await
        .with_context(|| format!("write {}", tmp))?;

    let backup = format!(
        "/tmp/archipelago-nginx-backup-{}-{}.conf",
        pid,
        patched.len()
    );
    if let Err(e) = host_sudo(&["cp", path, &backup]).await {
        let _ = fs::remove_file(&tmp).await;
        return Err(e.context("backup nginx conf"));
    }

    let mv = host_sudo(&["mv", &tmp, path]).await;
    match mv {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let _ = fs::remove_file(&tmp).await;
            anyhow::bail!("sudo mv nginx conf to {} exited with {}", path, s);
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp).await;
            return Err(e.context("mv tmp -> nginx conf"));
        }
    }

    // Validate.
    let test = host_sudo(&["nginx", "-t"]).await;
    let valid = matches!(&test, Ok(s) if s.success());
    if !valid {
        warn!("nginx -t failed after patch — reverting");
        let _ = host_sudo(&["mv", &backup, path]).await;
        if let Err(e) = test {
            return Err(e.context("nginx -t"));
        }
        anyhow::bail!("nginx config invalid after patch — reverted");
    }

    // Reload nginx so the new block takes effect immediately. Reload (not
    // restart) keeps in-flight connections alive.
    if let Err(e) = host_sudo(&["systemctl", "reload", "nginx"]).await {
        warn!("nginx reload failed (non-fatal): {:#}", e);
    }
    let _ = host_sudo(&["rm", "-f", &backup]).await;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_service_uses_the_canonical_ota_script_path() {
        let expected = format!("ExecStart={} --local", DOCTOR_SH_PATH);
        assert!(DOCTOR_SERVICE.lines().any(|line| line == expected));
        assert_eq!(
            DOCTOR_SH_PATH,
            "/opt/archipelago/scripts/container-doctor.sh"
        );
        assert!(!DOCTOR_SERVICE.contains("/home/archipelago/archy/"));
    }

    #[test]
    fn podman_heal_outcome_no_longer_has_cleanup_variant() {
        let outcome = PodmanHealOutcome::Unhealthy;
        assert_ne!(outcome, PodmanHealOutcome::Healthy);
    }

    #[test]
    fn stale_web_search_block_is_gated_and_idempotent() {
        let stale = "    location /aiui/api/web-search {\n        proxy_pass http://127.0.0.1:8888/search;\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_connect_timeout 30s;\n    }";
        let healed = heal_stale_web_search_block(stale).expect("stale block must heal");
        assert!(healed.contains("proxy_pass http://127.0.0.1:5678;"));
        assert!(healed.contains("proxy_set_header Cookie $http_cookie;"));
        assert!(!healed.contains("8888"));
        // Second pass is a no-op (idempotent self-heal).
        assert!(heal_stale_web_search_block(&healed).is_none());
        // A config without the block is untouched.
        assert!(
            heal_stale_web_search_block("location / { try_files $uri /index.html; }").is_none()
        );
    }

    #[test]
    fn source_proxy_uses_same_origin_through_authenticated_app_gate() {
        let main = "server {\n    location /app/gitea/ {\n    }\n}\nserver {\n    location /app/gitea/ {\n    }\n}";
        let healed = heal_missing_source_proxy(main).expect("source proxy must be added");
        assert_eq!(
            healed.matches("location /app/archipelago-source/").count(),
            2
        );
        assert!(healed.contains("proxy_pass http://127.0.0.2:8337/;"));
        assert!(healed.contains("proxy_set_header Cookie $http_cookie;"));
        assert!(healed.contains("proxy_set_header X-Forwarded-Prefix /app/archipelago-source;"));
        assert!(heal_missing_source_proxy(&healed).is_none());

        let snippet = "location /app/gitea/ {\n}";
        let healed = heal_missing_source_proxy(snippet).expect("snippet must be patched");
        assert!(healed.starts_with("# GitWorkshop follows the dashboard origin"));
    }

    #[test]
    fn existing_source_proxy_gets_the_forwarded_mount_once() {
        let stale = "location /app/archipelago-source/ {\n    proxy_pass http://127.0.0.2:8337/;\n    proxy_http_version 1.1;\n    proxy_set_header Host $http_host;\n    proxy_set_header Cookie $http_cookie;\n    proxy_set_header X-Real-IP $remote_addr;\n    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n    proxy_set_header X-Forwarded-Proto $scheme;\n    proxy_hide_header X-Frame-Options;\n}";
        let healed = heal_source_forwarded_prefix(stale).expect("mount header must be added");
        assert_eq!(
            healed
                .matches("X-Forwarded-Prefix /app/archipelago-source")
                .count(),
            1
        );
        assert!(heal_source_forwarded_prefix(&healed).is_none());
    }

    #[test]
    fn nostr_signer_is_added_to_each_dashboard_server_only_once() {
        let main =
            "server {\n    location /aiui/ {\n    }\n}\nserver {\n    location /aiui/ {\n    }\n}";
        let healed = heal_missing_nostr_signer(main).expect("signer route must be added");
        assert_eq!(healed.matches("location = /nostr-signer").count(), 2);
        assert!(healed.contains("frame-ancestors 'self' http://$host:* https://$host:*"));
        assert!(heal_missing_nostr_signer(&healed).is_none());
        assert!(heal_missing_nostr_signer("location /app/gitea/ {}\n").is_none());
    }

    /// The exact ExecStart framework-pt shipped with must parse, and the
    /// rewrite must preserve its listen port and forward target.
    #[test]
    fn static_socat_bind_is_parsed_and_rewritten_dynamically() {
        let line = "ExecStart=/usr/bin/socat TCP-LISTEN:18332,bind=192.168.1.249,fork,reuseaddr TCP:127.0.0.1:8332";
        let (port, target) = parse_socat_static_bind(line).expect("must parse");
        assert_eq!(port, "18332");
        assert_eq!(target, "127.0.0.1:8332");
        let dynamic = dynamic_bind_execstart(&port, &target);
        assert!(dynamic.contains("TCP-LISTEN:18332,bind=$$IP"));
        assert!(dynamic.contains("TCP:127.0.0.1:8332"));
        assert!(dynamic.contains("route get 1.1.1.1"));
        // The heal is idempotent: its own output no longer parses as a
        // static bind (bind=$$IP is not a concrete address).
        assert!(parse_socat_static_bind(&dynamic).is_none());
    }

    /// The archi-dev-box config: one stale address (old network) beside the
    /// WireGuard one. The stale listener must go — nginx refuses to START
    /// while it names an address the host lacks — and the current LAN
    /// address must appear.
    #[test]
    fn stale_https_listeners_are_retargeted_to_present_addresses() {
        let cfg = "server {\n    listen 80 default_server;\n    listen 10.44.0.1:443 ssl;\n    listen 192.168.63.240:443 ssl;\n    ssl_certificate /x;\n}\n";
        let present = vec!["10.44.0.1".to_string(), "192.168.1.50".to_string()];
        let healed = retarget_https_listeners(cfg, &present).expect("must heal");
        assert!(healed.contains("listen 192.168.1.50:443 ssl;"));
        assert!(healed.contains("listen 10.44.0.1:443 ssl;"));
        assert!(
            !healed.contains("192.168.63.240"),
            "stale listener must be dropped"
        );
        // Untouched lines survive, and the repair is idempotent.
        assert!(healed.contains("listen 80 default_server;"));
        assert!(healed.contains("ssl_certificate /x;"));
        assert!(retarget_https_listeners(&healed, &present).is_none());
    }

    #[test]
    fn wildcard_only_configs_and_cgnat_are_left_alone() {
        // No address-pinned listener → nothing to heal (the ISO's own config).
        assert!(retarget_https_listeners(
            "server {\n    listen 443 ssl default_server;\n}\n",
            &["192.168.1.50".to_string()]
        )
        .is_none());
        // Tailscale CGNAT must never become an nginx listener: tailscaled
        // already holds :443 there, and binding it would fail nginx outright.
        assert!(is_cgnat("100.69.68.39"));
        assert!(is_cgnat("100.127.255.1"));
        assert!(!is_cgnat("100.128.0.1"));
        assert!(!is_cgnat("192.168.1.50"));
        assert!(!is_cgnat("10.44.0.1"));
    }

    #[test]
    fn socat_units_that_need_no_heal_are_left_alone() {
        // Loopback bind is intentional (Tor bootstrap forwarder) — not ours.
        assert!(parse_socat_static_bind(
            "ExecStart=/usr/bin/socat TCP-LISTEN:18332,bind=127.0.0.1,reuseaddr,fork SOCKS4A:127.0.0.1:x.onion:8332,socksport=9050"
        )
        .is_none());
        // No bind at all.
        assert!(parse_socat_static_bind(
            "ExecStart=/usr/bin/socat TCP-LISTEN:18332,fork,reuseaddr TCP:127.0.0.1:8332"
        )
        .is_none());
        // Not a socat line.
        assert!(parse_socat_static_bind("ExecStart=/usr/bin/true").is_none());
    }
}

/// Repair this node's own systemd restart policy.
///
/// The in-process updater replaces the binary and then asks systemd to
/// restart the service, treating `Restart=always` on the unit as its second
/// net if that request is ever lost. On austin-sapien (2026-08-05) the unit
/// was an old one carrying `Restart=on-failure`: the daemon exited cleanly
/// (status 0), systemd read that as success, and the node sat dead for over
/// two hours after a routine update — "server starting" in the UI, with
/// nothing to start it.
///
/// A node cannot be relied on to fix this via `self-update.sh` (which does
/// refresh units) because the in-process update path never runs it. So the
/// daemon checks its own unit at boot: any node that starts even once ends
/// up with a policy that survives the next update. Deliberately narrow —
/// only the `Restart=` line is touched, so local edits elsewhere in the unit
/// are preserved.
pub async fn ensure_restart_policy() {
    const UNIT: &str = "/etc/systemd/system/archipelago.service";
    let Ok(body) = fs::read_to_string(UNIT).await else {
        return; // not a systemd install (container, dev box) — nothing to do
    };
    if !body.lines().any(|l| {
        let l = l.trim();
        l.starts_with("Restart=") && l != "Restart=always"
    }) {
        return; // already correct, or no Restart= line to repair
    }
    let patched: String = body
        .lines()
        .map(|l| {
            if l.trim().starts_with("Restart=") && l.trim() != "Restart=always" {
                "Restart=always"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    match write_root_if_needed(UNIT, &patched).await {
        Ok(true) => {
            tracing::warn!(
                "repaired archipelago.service Restart= policy to always — this node would \
                 have stayed dead after an in-process update"
            );
            let _ = host_sudo(&["systemctl", "daemon-reload"]).await;
        }
        Ok(false) => {}
        Err(e) => tracing::warn!(error = %e, "could not repair archipelago.service restart policy"),
    }
}
