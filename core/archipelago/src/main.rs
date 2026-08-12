// Archipelago Bitcoin Node OS - Native Backend
// Pure Archipelago implementation, no StartOS dependencies

// Crate-level clippy allowances. These are stylistic lints that fire on
// large legacy surfaces and offer no correctness benefit to chase on every
// PR — suppressing them crate-wide keeps CI gating on correctness issues
// without drowning in cleanup noise every time a new toolchain tightens.
#![allow(
    clippy::too_many_arguments,
    clippy::doc_lazy_continuation,
    clippy::type_complexity,
    clippy::enum_variant_names,
    clippy::wildcard_in_or_patterns,
    clippy::assertions_on_constants,
    clippy::drop_non_drop,
    clippy::unused_io_amount,
    clippy::ptr_arg
)]

use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::Notify;
use tracing::info;

mod api;
mod app_ops;
mod appgate;
mod assistant;
mod auth;
mod avatar;
mod backup;
mod bitcoin_rpc;
mod bitcoin_status;
mod blobs;
mod bootstrap;
mod ceremony;
mod config;
mod constants;
mod container;
mod content_hash;
mod content_indeehub;
mod content_invoice;
mod content_owned;
mod content_server;
mod crash_recovery;
mod credentials;
mod data_model;
mod device_tokens;
mod disk_monitor;
mod electrs_status;
mod entropy;
mod federation;
mod fips;
mod health_monitor;
mod host_ip;
mod identity;
mod identity_manager;
mod marketplace;
mod mesh;
mod mesh_ports;
mod monitoring;
mod music;
mod names;
mod network;
mod node_message;
mod nostr_discovery;
mod nostr_handshake;
mod nostr_relays;
mod peers;
mod port_allocator;
mod rate_limit;
pub mod seed;
mod server;
mod session;
mod settings;
mod state;
mod storage_crypto;
mod streaming;
mod swarm;
mod tollgate_sweep;
mod totp;
mod transport;
mod trust;
mod update;
mod vpn;
mod wallet;
mod webhooks;

use config::Config;
use container::{
    BootReconciler, ContainerOrchestrator, DevContainerOrchestrator, ProdContainerOrchestrator,
    RECONCILER_DEFAULT_INTERVAL,
};
use server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    // Release-root signing ceremony: a publisher-side subcommand of the same
    // binary. Handle it before any server/tracing init so its stdout stays
    // clean (machine-readable KEY=VALUE lines) and it never touches node state.
    if ceremony::is_ceremony_invocation() {
        return ceremony::run();
    }

    // Plain CLI flags must never boot the daemon (a stray `--version` used to
    // start a second instance next to the systemd one). Handled before any
    // tracing/state init so stdout stays clean.
    match std::env::args().nth(1).as_deref() {
        Some("--version") | Some("-V") => {
            println!(
                "archipelago {}-{}",
                env!("CARGO_PKG_VERSION"),
                option_env!("GIT_HASH").unwrap_or("dev")
            );
            return Ok(());
        }
        Some("--help") | Some("-h") => {
            println!("Archipelago Bitcoin Node OS");
            println!();
            println!("Usage: archipelago [COMMAND]");
            println!();
            println!("Running with no arguments starts the node daemon.");
            println!();
            println!("Commands:");
            println!("  ceremony <gen|pubkey|sign|verify>  Release-root signing ceremony");
            println!();
            println!("Options:");
            println!("  -V, --version  Print version and exit");
            println!("  -h, --help     Print this help and exit");
            return Ok(());
        }
        Some(other) if other.starts_with('-') => {
            eprintln!("archipelago: unknown option '{other}' (see --help)");
            std::process::exit(2);
        }
        _ => {}
    }

    let startup_start = std::time::Instant::now();
    crash_recovery::init_start_time();

    // Initialize tracing. Default to `info`: production units don't set
    // RUST_LOG, and the old `archipelago=debug` default flooded journald
    // with per-request debug lines ("RPC method: …", cookie-flag notes) —
    // part of a >1 GB/day journal on a fresh node. Set RUST_LOG (e.g.
    // RUST_LOG=archipelago=debug) to get debug logs back when debugging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    info!("Starting Archipelago Bitcoin Node OS");

    // Self-heal web-ui permissions. The OTA updater in <=v1.7.38 left
    // /opt/archipelago/web-ui as drwx------ (700) after the atomic
    // swap — nginx (www-data) then returned 500/403 on every request
    // until someone shelled in and chmod'd it. Check on every boot
    // and repair if needed so a node auto-recovers after the next
    // service restart that follows a broken OTA.
    tokio::spawn(async move {
        use std::os::unix::fs::PermissionsExt;
        let web_ui = std::path::Path::new("/opt/archipelago/web-ui");
        if let Ok(meta) = tokio::fs::metadata(web_ui).await {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o005 != 0o005 {
                tracing::warn!("web-ui perms {:o} not world-readable — self-healing", mode);
                let _ = tokio::process::Command::new("sudo")
                    .args([
                        "-n",
                        "chmod",
                        "-R",
                        "u=rwX,go=rX",
                        "/opt/archipelago/web-ui",
                    ])
                    .status()
                    .await;
            }
        }
    });

    // Load configuration
    let config = Config::load().await?;
    info!("📁 Data directory: {}", config.data_dir.display());

    // Load user transport preferences so peer-to-peer call sites can
    // consult them from any module without threading a handle through
    // deep async chains. Missing/corrupt file → default (Auto everywhere).
    if let Err(e) = settings::transport::init(&config.data_dir).await {
        tracing::warn!(
            "Failed to initialise transport preferences: {} — using defaults",
            e
        );
    }

    // Check for a crash marker BEFORE writing our own. The old order wrote
    // the marker first, so the check always read the CURRENT process's PID,
    // found it alive, and skipped recovery — on every boot, forever.
    let crash_containers = crash_recovery::check_for_crash(&config.data_dir).await;

    // Now mark this instance as running so the next startup can detect a crash.
    crash_recovery::write_pid_marker(&config.data_dir).await?;

    // Signal READY *before* the heavy synchronous boot recovery below. On a
    // node with many stacks that recovery takes minutes, and the unit sat in
    // `activating` the whole time — so anything that touched the service in
    // that window (a superseding start/restart, a start-timeout) killed a
    // half-started instance, which then exited 0 and (under the old
    // Restart=on-failure) never came back: "server starting up" forever,
    // reproduced on a test node installing apps on 2026-07-26. The daemon's
    // real work (recovery, reconcile, listener) continues after READY; being
    // "active" early is honest — the process is up and doing its job.
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]);
    // Watchdog pings must run DURING the long recovery too, or a slow boot
    // trips WatchdogSec. Spawn the keepalive here rather than after serve().
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
        loop {
            interval.tick().await;
            let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
        }
    });

    // Run crash recovery before starting the manifest reconciler. Both paths
    // mutate Podman; running them concurrently can corrupt transient runtime
    // state and leave netavark/conmon unable to start containers.
    match crash_containers {
        Ok(Some(containers)) => {
            info!(
                "🔧 Recovering {} containers from previous crash...",
                containers.len()
            );
            let report = crash_recovery::recover_containers(&containers).await;
            info!(
                "🔧 Recovery complete: {}/{} containers restarted (failed: {:?})",
                report.recovered, report.total, report.failed
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("Crash recovery check failed: {}", e);
        }
    }

    // Start any stopped containers (handles clean reboot). This remains
    // synchronous for the same reason: no concurrent reconciler during Podman
    // startup/recovery operations.
    let boot_report = crash_recovery::start_stopped_containers(&config.data_dir).await;
    if boot_report.total > 0 {
        info!(
            "🔄 Boot startup: {}/{} containers started (failed: {:?})",
            boot_report.recovered, boot_report.total, boot_report.failed
        );
    }
    // Construct the container orchestrator once. In prod mode we load the
    // on-disk app manifests, do an initial adoption pass, and spawn the
    // BootReconciler loop (Step 5/6 of the rust-orchestrator migration).
    // Dev mode uses the in-memory DevContainerOrchestrator and has no
    // reconciler (manifests are pushed via RPC, not discovered from disk).
    let shutdown_notify = Arc::new(Notify::new());
    let (orchestrator, dev_orchestrator): (
        Option<Arc<dyn ContainerOrchestrator>>,
        Option<Arc<DevContainerOrchestrator>>,
    ) = if config.dev_mode {
        let dev = Arc::new(DevContainerOrchestrator::new(config.clone()).await?);
        let trait_obj: Arc<dyn ContainerOrchestrator> = dev.clone();
        (Some(trait_obj), Some(dev))
    } else {
        let prod = Arc::new(ProdContainerOrchestrator::new(config.clone()).await?);
        // Pull the freshest signed app-catalog BEFORE loading manifests, so any
        // registry-embedded manifest (the origin-wins overlay in load_manifests)
        // is in place on THIS boot — not a restart later. Without this the boot
        // would overlay the previous run's cached catalog and a newly-published
        // app (e.g. a registry-only install) wouldn't appear until the next
        // restart. Bounded + best-effort: on timeout/unreachable origin the
        // last-cached catalog (or the disk manifests) still load — registry is
        // an overlay on top of disk, never a hard dependency.
        match tokio::time::timeout(
            std::time::Duration::from_secs(25),
            crate::container::app_catalog::refresh_catalog(&config.data_dir),
        )
        .await
        {
            Ok(Ok(r)) => info!(
                "🛰️  app-catalog refreshed before manifest load ({} apps)",
                r.apps
            ),
            Ok(Err(e)) => tracing::debug!("app-catalog pre-load refresh failed (using cache): {e}"),
            Err(_) => tracing::debug!("app-catalog pre-load refresh timed out (using cache)"),
        }
        // Best-effort manifest load; a missing /opt/archipelago/apps is
        // logged inside load_manifests and not fatal.
        match prod.load_manifests().await {
            Ok(n) => info!("📦 Loaded {n} app manifest(s) (disk + registry catalog)"),
            Err(e) => {
                tracing::error!(error = %e, "prod orchestrator: load_manifests failed at startup");
            }
        }
        // Reboot-survival safety net for the podman `--restart` path: ensure the
        // user's podman-restart.service is enabled so `unless-stopped` containers
        // come back after a reboot even when the Quadlet backend path is off
        // (orchestrator-installed backends like immich/btcpay run as plain podman
        // containers until the Phase-3 Quadlet rollout). Idempotent + best-effort.
        {
            let out = tokio::process::Command::new("systemctl")
                .args(["--user", "enable", "--now", "podman-restart.service"])
                .output()
                .await;
            match out {
                Ok(o) if o.status.success() => {
                    info!("🔁 podman-restart.service enabled (reboot-survival for --restart containers)")
                }
                Ok(o) => tracing::debug!(
                    "podman-restart.service enable skipped: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
                Err(e) => tracing::debug!("podman-restart.service enable skipped: {e}"),
            }
        }
        // Adoption pass: link existing podman containers back to their
        // manifests so the reconciler doesn't recreate them.
        match tokio::time::timeout(Duration::from_secs(35), prod.adopt_existing()).await {
            Ok(Ok(report)) => {
                info!(
                    "🔗 Adopted {} existing container(s): {:?}",
                    report.adopted.len(),
                    report.adopted
                );
                // Seed the durable installation record from what demonstrably
                // exists. Nodes that predate the record would otherwise start
                // with an empty one, which reads as "nothing is installed" and
                // disables desired-state recovery for precisely the apps that
                // need it. Additive and evidence-based: only names with a real
                // adopted container are claimed, and anything the operator
                // uninstalled is skipped, so it cannot invent an install.
                crate::crash_recovery::backfill_installed_apps(&config.data_dir, &report.adopted)
                    .await;
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "prod orchestrator: adopt_existing failed (non-fatal)");
            }
            Err(_) => {
                tracing::warn!("prod orchestrator: adopt_existing timed out after 35s (non-fatal)")
            }
        }
        // Spawn the boot reconciler loop. Runs an initial reconcile
        // immediately, then re-checks every RECONCILER_DEFAULT_INTERVAL
        // until shutdown_notify fires.
        {
            let reconciler = BootReconciler::new(
                prod.clone(),
                RECONCILER_DEFAULT_INTERVAL,
                shutdown_notify.clone(),
            );
            tokio::spawn(reconciler.run_forever());
            info!(
                "🔄 Boot reconciler started (interval: {:?})",
                RECONCILER_DEFAULT_INTERVAL
            );
        }
        let trait_obj: Arc<dyn ContainerOrchestrator> = prod;
        (Some(trait_obj), None)
    };

    // Ensure a default user exists so login works after install/onboarding.
    // In production, the default password is "password123" (shown during install).
    // In dev mode, the dev default password is used.
    // Don't auto-create default user — let onboarding flow handle password setup
    // via auth.setup RPC. The Login page detects is_setup=false and shows
    // "Create Password" form instead of login form.

    // Create server. Keep a clone of the orchestrator handle for the background
    // update scheduler (per-app auto-update applies via the orchestrator).
    let update_orchestrator = orchestrator.clone();
    let server = Server::new(config.clone(), orchestrator, dev_orchestrator).await?;

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.bind_host, config.bind_port)
        .parse()
        .context("Invalid bind address")?;

    // The FIPS peer listener is bound lazily by server::serve_with_shutdown
    // on a 30s poll of fips0 — so a post-onboarding fips.install brings it
    // online without needing an archipelago restart.

    // Post-OTA verification: if apply_update() wrote a pending-verify
    // marker right before the restart, probe the frontend now and auto-
    // rollback if it's broken. This is the guardrail that stops fleet-
    // wide breakage when an OTA lands a subtly-bad release (v1.7.38/39
    // tarball-perms → nginx 500 was the trigger). Runs concurrently
    // with normal startup — doesn't delay the server coming up.
    {
        let data_dir = config.data_dir.clone();
        tokio::spawn(async move {
            update::verify_pending_update(&data_dir).await;
        });
    }

    // Spawn background update scheduler. Pass the orchestrator so the scheduler
    // can apply per-app auto-update-to-latest (multi-version support) via the
    // safe orchestrator upgrade path; None in dev mode disables it.
    let update_data_dir = config.data_dir.clone();
    tokio::spawn(async move {
        update::run_update_scheduler(update_data_dir, update_orchestrator).await;
    });

    // Synchronize host-side doctor artifacts (script + systemd units) with
    // what's embedded in this binary. Runs in the background so it never
    // delays server readiness; best-effort, warnings only.
    tokio::spawn(bootstrap::ensure_doctor_installed());

    // B17: heal already-deployed nodes whose archipelago.service lacks a mount
    // dependency on the data volume, so cold boots stop flapping. Boot-ordering
    // only — effective next reboot; never restarts the running service.
    tokio::spawn(bootstrap::ensure_archipelago_mount_ordering());

    // #36: keep the kiosk unit + launcher hardened (CPU/mem cap + GPU-vs-headless
    // flags) on already-deployed nodes via OTA; no-op if the kiosk isn't installed.
    tokio::spawn(bootstrap::ensure_kiosk_hardened());

    // Repair our own restart policy before anything else can need it: a node
    // whose unit still says Restart=on-failure stays dead after the next
    // in-process update, because the daemon exits cleanly to be restarted.
    tokio::spawn(bootstrap::ensure_restart_policy());

    // HDMI audio: install the PipeWire stack + audio-router daemon on kiosk
    // nodes (older ISOs shipped no audio stack; the router also heals the
    // boot-time ELD race that leaves HDMI silently unavailable).
    tokio::spawn(bootstrap::ensure_audio_stack());

    // TV input: gamepad→keyboard bridge so controllers work inside every app
    // iframe on kiosk nodes (docs/tv-input-iframe-apps.md).
    tokio::spawn(bootstrap::ensure_gamepad_keys());

    // Mesh access: mirror IPv4-published app ports onto [::] so direct-port
    // app URLs (http://[<fips0 ULA>]:<port>) work from the companion.
    tokio::spawn(mesh_ports::run_mesh_port_mirror());

    // Pine voice: re-point IP-pinned Wyoming satellite entries (speakers) when
    // DHCP renumbering strands them — HA never re-resolves on its own.
    tokio::spawn(api::rpc::wyoming_satellite_keeper());

    // Spawn periodic container snapshot (for crash recovery)
    crash_recovery::spawn_snapshot_task(config.data_dir.clone());

    // Recover stopped multi-container stack members after the backend is up.
    // This can take minutes on busy nodes after a reboot, so keep it out of
    // the synchronous systemd startup path.
    {
        let data_dir = config.data_dir.clone();
        tokio::spawn(async move {
            let report = crash_recovery::start_stopped_stack_containers(&data_dir).await;
            if report.total > 0 {
                info!(
                    "🔄 Stack boot recovery: {}/{} containers started (failed: {:?})",
                    report.recovered, report.total, report.failed
                );
            }
            crash_recovery::mark_recovery_complete();
        });
    }

    // Periodically restart crashed multi-container stack members (immich,
    // indeedhub, …) at RUNTIME, not just at boot. The health monitor skips them
    // as "orphans" because the sub-container app_ids (e.g. immich_server) aren't
    // in package_data, so without this a crashed immich_server / indeedhub-api
    // never comes back until the next reboot (#16/#17). Reuses the boot
    // recovery, which cheaply skips already-running containers and respects the
    // user-stopped list, so this only acts on genuinely-down stack members.
    {
        let data_dir = config.data_dir.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(120));
            tick.tick().await; // consume the immediate tick; boot recovery covers t0
            loop {
                tick.tick().await;
                let report = crash_recovery::start_stopped_stack_containers(&data_dir).await;
                if report.recovered > 0 {
                    info!(
                        "🔄 Stack supervisor: restarted {} crashed stack member(s) (failed: {:?})",
                        report.recovered, report.failed
                    );
                }
            }
        });
    }

    // Spawn disk space monitor (warns at 85%, auto-cleans at 90%)
    disk_monitor::spawn_disk_monitor(config.data_dir.clone());

    // Restore WireGuard peers into wg0 (kernel loses them on every reboot).
    {
        let data_dir = config.data_dir.clone();
        tokio::spawn(async move {
            vpn::restore_wg_peers(&data_dir).await;
        });
    }

    // Spawn ElectrumX status cache (refreshes every 15s, serves cached data to avoid race conditions)
    electrs_status::spawn_status_cache();
    bitcoin_status::spawn_status_cache();

    // FIPS supervisor: auto-activate FIPS after onboarding (no Activate button
    // needed) and keep hole-punched paths to federation peers warm so peer dials
    // land on FIPS (the preferred transport) instead of falling back to Tor.
    fips::spawn_fips_supervisor(config.data_dir.clone());

    let startup_ms = startup_start.elapsed().as_millis();
    info!(
        "Server listening on http://{} (startup: {}ms)",
        addr, startup_ms
    );
    info!("RPC API: http://{}/rpc/v1", addr);
    info!("WebSocket: ws://{}/ws", addr);

    // Notify systemd that we're ready (Type=notify)
    // Note: first param `false` keeps NOTIFY_SOCKET so watchdog pings work
    // READY + watchdog keepalive were already signalled/spawned earlier
    // (before boot recovery) so the unit reaches `active` in seconds instead
    // of sitting in `activating` through a minutes-long recovery.

    // Graceful shutdown: wait for SIGTERM or SIGINT
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .context("Failed to register SIGTERM handler")?;
    let shutdown_notify_for_signal = shutdown_notify.clone();
    let shutdown = async move {
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown...");
            }
        }
        // Signal the boot reconciler (and any other subscribers) to stop.
        // `notify_one` stores a permit if no task is currently parked on
        // `notified()`, so we don't race the reconciler's reconcile_all pass.
        shutdown_notify_for_signal.notify_one();
    };

    server.serve_with_shutdown(addr, shutdown).await?;

    // Clean shutdown: remove PID marker so next startup doesn't trigger recovery
    crash_recovery::remove_pid_marker(&config.data_dir).await;

    info!("Archipelago shut down cleanly");

    // Hard-exit after logging. All business state is persisted by now
    // (connections drained, PID marker removed, disk flushes done via
    // tokio::fs awaits). Letting tokio try to drop the runtime instead
    // can stall for 15s+ on non-daemon OS threads we don't directly
    // own (mdns_sd daemon, reqwest resolver pool, etc.) — long enough
    // for systemd's TimeoutStopSec to SIGKILL us and mark the service
    // Failed, which makes an otherwise-successful update look like a
    // crash in `systemctl status`.
    std::process::exit(0);
}
