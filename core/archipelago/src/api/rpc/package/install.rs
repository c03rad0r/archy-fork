use super::config::{
    get_app_capabilities, get_app_config, get_health_check_args, get_memory_limit,
    is_readonly_compatible, is_valid_docker_image,
};
use super::dependencies::{
    check_bitcoin_pruning_compatibility, check_cuprate_disk_compatibility, configure_fedimint_lnd,
    detect_existing_containers, detect_running_deps, detect_running_deps_from_package_data,
    log_optional_dep_info, needs_archy_net, wait_for_install_deps, DepProbe, RunningDeps,
    DEP_WAIT_INTERVAL, DEP_WAIT_MAX_ATTEMPTS,
};
use super::progress::parse_pull_progress;
use super::validation::validate_app_id;
use crate::api::rpc::RpcHandler;
use crate::data_model::InstallPhase;
use crate::update::host_sudo;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};

const INSTALL_LOG: &str = "/var/log/archipelago/container-installs.log";
const IMAGE_INSPECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Append a timestamped line to the persistent install log.
pub(in crate::api::rpc) async fn install_log(msg: &str) {
    use tokio::io::AsyncWriteExt;
    // Always mirror to tracing/journald: the file append below has been
    // silently failing under the service sandbox (ProtectSystem=strict
    // leaves /var/log/archipelago read-only — container-installs.log has
    // been 0 bytes since April 2026), and these lifecycle breadcrumbs are
    // the primary forensic trail for gate failures.
    info!(target: "install_log", "{msg}");
    let ts = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let line = format!("[{}] {}\n", ts, msg);
    if let Ok(mut f) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(INSTALL_LOG)
        .await
    {
        // Fire-and-forget by design: install-log lines are advisory, and a
        // per-line warn would spam the very log stream that's failing.
        let _ = f.write_all(line.as_bytes()).await;
    }
}

async fn local_podman_image_exists(image: &str) -> Result<bool> {
    let mut cmd = tokio::process::Command::new("podman");
    cmd.args(["image", "inspect", image]);
    cmd.kill_on_drop(true);
    let output = timeout(IMAGE_INSPECT_TIMEOUT, cmd.output())
        .await
        .with_context(|| {
            format!(
                "podman image inspect {} timed out after {}s",
                image,
                IMAGE_INSPECT_TIMEOUT.as_secs()
            )
        })?
        .with_context(|| format!("Failed to execute podman image inspect {}", image))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(code) => Err(anyhow::anyhow!(
            "podman image inspect {} exited with {}: {}",
            image,
            code,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        None => Err(anyhow::anyhow!(
            "podman image inspect {} terminated by signal",
            image
        )),
    }
}

fn patched_indeedhub_nginx_config(original: &str) -> String {
    let mut conf = original
        .lines()
        .filter(|line| !line.contains("X-Frame-Options"))
        .collect::<Vec<_>>()
        .join("\n");
    conf.push('\n');
    if !conf.contains("location = /nostr-provider.js {") {
        conf = conf.replace(
            "location = /sw.js {",
            "location = /nostr-provider.js {\n\
                    add_header Cache-Control \"no-cache, no-store, must-revalidate\";\n\
                    expires off;\n\
                }\n\n\
                location = /sw.js {",
        );
    }
    if conf.contains("try_files") && !conf.contains("sub_filter") {
        conf = conf.replacen(
            "try_files $uri $uri/ /index.html;",
            "try_files $uri $uri/ /index.html;\n\
                        sub_filter_once on;\n\
                        sub_filter '</head>' '<script src=\"/nostr-provider.js?v=tab-signer-v4\"></script></head>';",
            1,
        );
    }
    conf = conf.replace(
        "src=\"/nostr-provider.js\"",
        "src=\"/nostr-provider.js?v=tab-signer-v4\"",
    );
    conf = conf.replace("tab-signer-v2", "tab-signer-v4");
    conf = conf.replace("tab-signer-v3", "tab-signer-v4");
    conf.replace(
        "proxy_set_header X-Forwarded-Prefix /api;",
        "proxy_set_header X-Forwarded-Prefix $http_x_forwarded_prefix/api;",
    )
}

pub(crate) async fn patch_indeedhub_nostr_provider() {
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Frontend assets can change during a dashboard-only OTA while the
    // IndeedHub container keeps running. Reconcile the injected provider on
    // daemon startup as well as app install/start, but stay quiet when the app
    // is not installed or is intentionally stopped.
    let running = tokio::process::Command::new("podman")
        .args(["inspect", "-f", "{{.State.Running}}", "indeedhub"])
        .output()
        .await
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true")
        .unwrap_or(false);
    if !running {
        return;
    }

    // `podman exec` cannot always join a rootless container's delegated cgroup
    // from the system service, while Podman 5's copier refuses to overwrite an
    // existing regular file. Mount the rootless storage namespace instead;
    // this replaces both files without entering the container's cgroup.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tmp_dir = format!("/tmp/indeedhub-nginx-patch-{}-{unique}", std::process::id());
    let tmp_path = format!("{tmp_dir}/default.conf");
    if tokio::fs::create_dir(&tmp_dir).await.is_err() {
        tracing::warn!("IndeeHub signer reconciliation could not create its temporary directory");
        return;
    }

    let mount_out = tokio::process::Command::new("podman")
        .args(["unshare", "podman", "mount", "indeedhub"])
        .output()
        .await;
    let container_root = mount_out
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|path| {
            std::path::Path::new(path).is_absolute()
                && path.contains("/containers/storage/overlay/")
                && path.ends_with("/merged")
        });
    let Some(container_root) = container_root else {
        let _ = tokio::fs::remove_dir(&tmp_dir).await;
        tracing::warn!("IndeeHub signer reconciliation could not mount rootless storage");
        return;
    };

    let provider_src = "/opt/archipelago/web-ui/nostr-provider.js";
    let provider_dest = format!("{container_root}/usr/share/nginx/html/nostr-provider.js");
    let provider_copied = tokio::fs::metadata(provider_src).await.is_ok()
        && tokio::process::Command::new("podman")
            .args([
                "unshare",
                "install",
                "-m",
                "644",
                provider_src,
                &provider_dest,
            ])
            .output()
            .await
            .map(|out| out.status.success())
            .unwrap_or(false);

    let copy_out = tokio::process::Command::new("podman")
        .args(["cp", "indeedhub:/etc/nginx/conf.d/default.conf", &tmp_path])
        .output()
        .await;

    let mut config_copied = false;
    if let Ok(out) = copy_out {
        if out.status.success() {
            if let Ok(original) = tokio::fs::read_to_string(&tmp_path).await {
                let conf = patched_indeedhub_nginx_config(&original);
                if conf != original && tokio::fs::write(&tmp_path, &conf).await.is_ok() {
                    config_copied = tokio::process::Command::new("podman")
                        .args([
                            "unshare",
                            "install",
                            "-m",
                            "644",
                            &tmp_path,
                            &format!("{container_root}/etc/nginx/conf.d/default.conf"),
                        ])
                        .output()
                        .await
                        .map(|out| out.status.success())
                        .unwrap_or(false);
                    if config_copied {
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        config_copied = tokio::process::Command::new("podman")
                            .args(["cp", "indeedhub:/etc/nginx/conf.d/default.conf", &tmp_path])
                            .output()
                            .await
                            .map(|out| out.status.success())
                            .unwrap_or(false)
                            && tokio::fs::read_to_string(&tmp_path)
                                .await
                                .map(|actual| actual == conf)
                                .unwrap_or(false);
                    }
                } else if conf == original
                    && conf.contains("location = /nostr-provider.js {")
                    && conf.contains("src=\"/nostr-provider.js?v=tab-signer-v4\"")
                {
                    config_copied = true;
                }
            }
        }
    }
    let _ = tokio::fs::remove_file(&tmp_path).await;
    let _ = tokio::fs::remove_dir(&tmp_dir).await;
    let _ = tokio::process::Command::new("podman")
        .args(["unshare", "podman", "unmount", "indeedhub"])
        .output()
        .await;

    let reload = tokio::process::Command::new("podman")
        .args(["kill", "--signal", "HUP", "indeedhub"])
        .output()
        .await;
    match reload {
        Ok(o) if o.status.success() && provider_copied && config_copied => {
            info!("IndeeHub: NIP-07 provider injected, nginx patched and reloaded");
        }
        Ok(o) => {
            tracing::warn!(
                "IndeeHub signer reconciliation incomplete (provider_copied={}, config_copied={}): {}",
                provider_copied,
                config_copied,
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(e) => {
            tracing::warn!("IndeeHub nginx reload error: {}", e);
        }
    }
}

fn dependency_cache_satisfies(package_id: &str, deps: &RunningDeps) -> bool {
    match package_id {
        "electrumx" | "mempool-electrs" | "electrs" | "lnd" | "btcpay-server" | "btcpayserver" => {
            deps.has_bitcoin
        }
        "mempool" | "mempool-web" => deps.has_bitcoin && deps.has_electrumx,
        "fedimint" => true,
        _ => true,
    }
}

impl RpcHandler {
    /// Install a package from a Docker image.
    /// Security: Image verification, resource limits, network isolation.
    pub(in crate::api::rpc) async fn handle_package_install(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let package_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?;
        validate_app_id(package_id)?;

        let docker_image = params
            .get("dockerImage")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing dockerImage"))?;

        install_log(&format!(
            "INSTALL START: {} (image: {})",
            package_id, docker_image
        ))
        .await;
        debug!(
            "Installing package {} from image {}",
            package_id, docker_image
        );

        cleanup_stale_package_ports(package_id).await;

        if !is_valid_docker_image(docker_image) {
            install_log(&format!(
                "INSTALL FAIL: {} — invalid image format",
                package_id
            ))
            .await;
            return Err(anyhow::anyhow!("Invalid Docker image format"));
        }

        // Save dynamic app config if provided by frontend (from remote catalog)
        // This allows new apps to be installed without hardcoding config in Rust.
        if let Some(config) = params.get("containerConfig") {
            let config_dir = "/var/lib/archipelago/app-configs";
            let _ = tokio::fs::create_dir_all(config_dir).await;
            let config_path = format!("{}/{}.json", config_dir, package_id);
            if let Err(e) = tokio::fs::write(&config_path, config.to_string()).await {
                tracing::warn!("Failed to save dynamic config for {}: {}", package_id, e);
            } else {
                tracing::info!("Saved dynamic app config for {} from catalog", package_id);
            }
        }

        // Multi-version support: honor an install-time version selection for the
        // orchestrator-managed Bitcoin apps. Selecting the catalog default (or
        // omitting `version`) leaves the app unpinned (tracks latest); selecting
        // an older version pins it so install_fresh resolves that image and the
        // update badge stays suppressed. See docs/bitcoin-multi-version-design.md.
        if matches!(package_id, "bitcoin-core" | "bitcoin-knots") {
            if let Some(value) = params.get("prune") {
                let prune = value.as_bool().context("prune must be a boolean")?;
                crate::settings::bitcoin_storage::save(&self.config.data_dir, prune).await?;
            }
            if let Some(version) = params.get("version").and_then(|v| v.as_str()) {
                persist_install_version_selection(package_id, version).await;
            }
        }

        // Phase: Preparing — emit BEFORE the stack dispatch so multi-container
        // stacks also flip state to Installing immediately. Without this, the
        // backend's package state for stack apps stayed empty until the first
        // podman pull finished, so a hard refresh during the early seconds of
        // a stack install showed the app as missing entirely (the user
        // reported "the app disappears from installing if you hard refresh
        // then sometimes comes back later").
        self.set_install_phase(package_id, InstallPhase::Preparing)
            .await;

        if matches!(package_id, "mempool" | "mempool-web") {
            self.gate_install_deps(package_id).await?;
            check_bitcoin_pruning_compatibility(package_id).await?;
        }

        // Multi-container stacks get their own install path
        if package_id == "immich" {
            return self.install_immich_stack().await;
        }
        if matches!(package_id, "btcpay-server" | "btcpayserver" | "btcpay") {
            return self.install_btcpay_stack().await;
        }
        if matches!(package_id, "mempool" | "mempool-web") {
            return self.install_mempool_stack().await;
        }
        if package_id == "indeedhub" {
            return self.install_indeedhub_stack().await;
        }
        if package_id == "netbird" {
            return self.install_netbird_stack().await;
        }
        if package_id == "pine" {
            return self.install_pine_stack().await;
        }
        // Dependency checks. Prefer the scanner's cached package state so a
        // congested Podman API does not turn an already-running dependency into
        // a false install failure. Fall back to a bounded direct Podman probe
        // only when the cache does not show the dependency. When the dependency
        // is installed but not Running yet (the "clicked Install LND 55s before
        // Bitcoin was up" race), wait up to ~3 minutes for it instead of
        // failing instantly.
        let deps = self.gate_install_deps(package_id).await?;
        check_bitcoin_pruning_compatibility(package_id).await?;
        check_cuprate_disk_compatibility(package_id).await?;
        log_optional_dep_info(package_id, &deps);
        if matches!(package_id, "bitcoin" | "bitcoin-core" | "bitcoin-knots") {
            // Materialise the RPC password file before any install path
            // runs. The orchestrator path resolves secret_env from
            // /var/lib/archipelago/secrets/bitcoin-rpc-password at start
            // time; if the file is missing, bitcoind exits within ms.
            // bitcoin_rpc_credentials() generates + persists on first
            // call (OnceCell-cached), so this is idempotent.
            let _ = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
            // A stale datadir bitcoin.conf from an older install conflicts
            // with the container's -conf=/tmp/rpc.conf launch (see
            // apps/bitcoin-core & bitcoin-knots manifest.yml) and makes
            // Bitcoin Core refuse to start at all. Clear it before
            // (re)install. Unlike the old bind-setting "repair" this was
            // replacing, it never requires restarting an already-running
            // container — bitcoind doesn't read this file, so removing it
            // changes nothing at runtime.
            remove_stale_bitcoin_conf().await?;
        }

        // For orchestrator-managed apps, skip the legacy "container exists →
        // adopt + return" probe entirely. The orchestrator's own install path
        // (below) calls ensure_running which:
        //   - no-ops if the container is already up and healthy,
        //   - removes + reinstalls if the container is broken,
        //   - actually verifies health via the manifest's health check
        //     (whereas the legacy adopt block returns success on a podman
        //     `start` exit-0, even if the process inside crashed seconds
        //     later — the .228 bitcoin "running but unreachable" failure
        //     mode).
        // The adoption block is being phased out as apps move to the
        // orchestrator path. Non-orchestrator apps still hit it.
        let orchestrator_managed = match self.orchestrator.as_ref() {
            // Migration allowlist OR any app whose manifest the orchestrator
            // knows (disk or signed-catalog overlay). A manifest-driven app
            // must never fall through to the legacy flow — it ignores the
            // manifest entirely and creates a bare container with no ports or
            // volumes (strfry, 2026-07-09).
            Some(orch) => {
                should_try_orchestrator_install(package_id, true)
                    || orch
                        .knows_app(orchestrator_install_app_id(package_id))
                        .await
            }
            None => false,
        };

        // Check if container already exists (legacy adoption — non-orchestrator
        // apps only).
        if !orchestrator_managed {
            let check_output = tokio::process::Command::new("podman")
                .args([
                    "ps",
                    "-a",
                    "--format",
                    "{{.Names}}",
                    "--filter",
                    &format!("name=^{}$", package_id),
                ])
                .output()
                .await
                .context("Failed to check existing containers")?;

            if !String::from_utf8_lossy(&check_output.stdout)
                .trim()
                .is_empty()
            {
                // Container already exists (e.g. created by first-boot) — adopt it
                info!(
                    "Container {} already exists, adopting as installed",
                    package_id
                );
                install_log(&format!(
                    "INSTALL ADOPT: {} — container already exists",
                    package_id
                ))
                .await;

                // Check container state
                let state_output = tokio::process::Command::new("podman")
                    .args(["inspect", package_id, "--format", "{{.State.Status}}"])
                    .output()
                    .await
                    .context("Failed to inspect existing container")?;
                let state = String::from_utf8_lossy(&state_output.stdout)
                    .trim()
                    .to_string();

                if state != "running" {
                    // Start the stopped/exited container
                    info!("Starting existing container {} (was {})", package_id, state);
                    let start_output = tokio::process::Command::new("podman")
                        .args(["start", package_id])
                        .output()
                        .await
                        .context("Failed to start existing container")?;
                    if !start_output.status.success() {
                        let stderr = String::from_utf8_lossy(&start_output.stderr);
                        install_log(&format!(
                            "INSTALL ADOPT RECREATE: {} — start failed, removing wedged container: {}",
                            package_id, stderr
                        ))
                        .await;
                        let _ = tokio::process::Command::new("podman")
                            .args(["rm", "-f", package_id])
                            .output()
                            .await;
                    } else {
                        wait_for_adopted_container(package_id, package_id).await?;
                        install_log(&format!(
                            "INSTALL ADOPT OK: {} — already running",
                            package_id
                        ))
                        .await;
                        if let Err(e) = ensure_host_port_listener(package_id, package_id, &[]).await
                        {
                            install_log(&format!(
                                "INSTALL ADOPT RECREATE: {} — host listener repair failed, removing wedged container: {}",
                                package_id, e
                            ))
                            .await;
                            let _ = tokio::process::Command::new("podman")
                                .args(["rm", "-f", package_id])
                                .output()
                                .await;
                        } else {
                            return Ok(serde_json::json!({
                                "success": true,
                                "package_id": package_id,
                                "message": format!("Package {} already installed and running", package_id)
                            }));
                        }
                    }
                    // Fall through to the fresh install path. Volume-backed data
                    // remains under /var/lib/archipelago and is not deleted.
                } else {
                    install_log(&format!(
                        "INSTALL ADOPT OK: {} — already running",
                        package_id
                    ))
                    .await;
                    if let Err(e) = ensure_host_port_listener(package_id, package_id, &[]).await {
                        install_log(&format!(
                            "INSTALL ADOPT RECREATE: {} — host listener repair failed, removing wedged container: {}",
                            package_id, e
                        ))
                        .await;
                        let _ = tokio::process::Command::new("podman")
                            .args(["rm", "-f", package_id])
                            .output()
                            .await;
                    } else {
                        return Ok(serde_json::json!({
                            "success": true,
                            "package_id": package_id,
                            "message": format!("Package {} already installed and running", package_id)
                        }));
                    }
                }
            }
        }

        // Preferred path for apps already modeled in the production orchestrator.
        // Keep legacy install flow as default while migration is in progress.
        if orchestrator_managed {
            let orchestrator_app_id = orchestrator_install_app_id(package_id);
            self.set_install_phase(package_id, InstallPhase::PreparingApp)
                .await;
            install_log(&format!(
                "INSTALL ORCH: {} — attempting orchestrator install as {}",
                package_id, orchestrator_app_id
            ))
            .await;

            if let Some(orchestrator) = self.orchestrator.as_ref() {
                match orchestrator.install(orchestrator_app_id).await {
                    Ok(container_name) => {
                        self.set_install_phase(package_id, InstallPhase::WaitingHealthy)
                            .await;
                        ensure_host_port_listener(package_id, &container_name, &[]).await?;
                        crate::api::rpc::package::runtime::reconcile_companions_for(package_id)
                            .await;
                        install_log(&format!(
                            "INSTALL ORCH OK: {} (app={}) — container={}",
                            package_id, orchestrator_app_id, container_name
                        ))
                        .await;
                        return Ok(serde_json::json!({
                            "success": true,
                            "package_id": package_id,
                            "container_name": container_name,
                            "message": format!("Package {} installed and started", package_id)
                        }));
                    }
                    Err(e) if is_unknown_app_id_error(&e) => {
                        info!(
                            "Install {}: orchestrator has no manifest mapping yet, falling back to legacy installer",
                            package_id
                        );
                        install_log(&format!(
                            "INSTALL ORCH SKIP: {} — unknown app_id, using legacy flow",
                            package_id
                        ))
                        .await;
                    }
                    Err(e) => {
                        install_log(&format!("INSTALL ORCH FAIL: {} — {}", package_id, e)).await;
                        return Err(
                            e.context(format!("Orchestrator install {} failed", package_id))
                        );
                    }
                }
            }
        }

        // Pull or verify image
        install_log(&format!(
            "INSTALL PULL: {} — pulling image {}",
            package_id, docker_image
        ))
        .await;
        // Phase: PullingImage — the longest phase. Podman doesn't emit
        // parseable progress on a piped stderr, so the UI shows an
        // indeterminate "Downloading image…" at this fixed percentage
        // until pull completes.
        self.set_install_phase(package_id, InstallPhase::PullingImage)
            .await;
        let has_local_fallback = self.pull_or_verify_image(package_id, docker_image).await?;
        install_log(&format!(
            "INSTALL PULL OK: {} — image ready (local_fallback={})",
            package_id, has_local_fallback
        ))
        .await;
        // Phase: CreatingContainer — image is local, now writing configs,
        // data directories, chowning to container UID, building the run
        // argv. Fast (sub-second to a few seconds).
        self.set_install_phase(package_id, InstallPhase::CreatingContainer)
            .await;

        // Normalize container name for legacy aliases
        let container_name = match package_id {
            "electrs" | "mempool-electrs" => "electrumx",
            _ => package_id,
        };

        // Read Bitcoin RPC credentials for container configs
        let (rpc_user, rpc_pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;

        // App-specific configuration
        let (mut ports, mut volumes, env_vars, custom_command, mut custom_args) = {
            let mut allocator = self.port_allocator.lock().await;
            get_app_config(
                package_id,
                &self.config.host_ip,
                &mut allocator,
                &rpc_user,
                &rpc_pass,
            )
            .await?
        };

        // Fedimint Gateway: auto-detect LND and switch to lnd mode
        if package_id == "fedimint-gateway" && deps.has_lnd {
            // get_app_config's "fedimint-gateway" arm already called
            // ensure_gateway_credential above, so the secret is guaranteed to
            // exist here; re-reading it (rather than threading the value
            // through) keeps one canonical read site in container::secrets.
            let fedi_hash = crate::container::secrets::gateway_bcrypt_hash(std::path::Path::new(
                "/var/lib/archipelago/secrets",
            ))?;
            configure_fedimint_lnd(
                &self.config.host_ip,
                &mut ports,
                &mut volumes,
                &mut custom_args,
                &rpc_user,
                &rpc_pass,
                &fedi_hash,
            );
        }

        // Build the podman run command
        let mut run_args = vec![
            "run",
            "-d",
            "--name",
            container_name,
            "--restart=unless-stopped",
        ];

        // Explicit DNS alias for aardvark-dns (must outlive run_args)
        let network_alias_flag = format!("--network-alias={}", container_name);

        // Network mode
        if matches!(
            package_id,
            "uptime-kuma"
                | "gitea"
                | "tailscale"
                | "vaultwarden"
                | "homeassistant"
                | "home-assistant"
                | "nextcloud"
                | "searxng"
                | "jellyfin"
                | "nginx-proxy-manager"
                | "portainer"
        ) {
            // These standalone web UIs have repeatedly lost host listeners
            // under Podman's rootless pasta backend while staying healthy internally.
            // Use slirp4netns/rootlessport for this standalone web UI.
            run_args.push("--network=slirp4netns:allow_host_loopback=true");
        } else if needs_archy_net(package_id) {
            // Create archy-net if it doesn't exist (idempotent — "already exists" is fine)
            match tokio::process::Command::new("podman")
                .args(["network", "create", "archy-net"])
                .output()
                .await
            {
                Ok(output) if !output.status.success() => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stderr.contains("already exists") {
                        tracing::warn!("Failed to create archy-net: {}", stderr.trim());
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to run podman network create: {}", e);
                }
                _ => {}
            }
            // Verify the network actually exists before using it
            let net_check = tokio::process::Command::new("podman")
                .args(["network", "exists", "archy-net"])
                .status()
                .await;
            if net_check.map(|s| s.success()).unwrap_or(false) {
                run_args.push("--network=archy-net");
                run_args.push(&network_alias_flag);
            } else {
                tracing::error!(
                    "archy-net network does not exist — {} will use default network. \
                     Inter-container DNS will not work.",
                    package_id
                );
            }
        }

        // DNS: ensure host.containers.internal resolves (needed for Tor proxy, inter-service calls)
        // Rootless podman 4.3.x doesn't support "host-gateway" — resolve to actual gateway IP
        let host_gateway_flag = resolve_host_gateway().await;
        run_args.push(&host_gateway_flag);

        // Security hardening (skip for privileged containers)
        let security_caps: Vec<String> = get_app_capabilities(package_id);
        let readonly_compatible = is_readonly_compatible(package_id);

        run_args.push("--cap-drop=ALL");
        run_args.push("--security-opt=no-new-privileges:true");
        run_args.push("--pids-limit=4096");
        for cap in &security_caps {
            run_args.push(cap);
        }
        if readonly_compatible {
            run_args.push("--read-only");
            run_args.push("--tmpfs=/tmp:rw,noexec,nosuid,size=256m");
            run_args.push("--tmpfs=/run:rw,noexec,nosuid,size=64m");
        }

        // Jellyfin: .NET CoreCLR needs exec-enabled /tmp for JIT compilation
        if package_id == "jellyfin" {
            run_args.push("--tmpfs=/tmp:rw,exec,size=256m");
        }

        // Create data directories (mkdir only — chown happens AFTER config files are written)
        for volume in &volumes {
            if let Some(host_path) = volume.split(':').next() {
                if host_path.starts_with("/var/lib/archipelago/") {
                    if let Err(e) = std::fs::create_dir_all(host_path) {
                        tracing::warn!("Failed to create directory {}: {}", host_path, e);
                    }
                }
            }
        }

        // Pre-install: clear a stale datadir bitcoin.conf BEFORE chown (dir is
        // still owned by archipelago user). bitcoind is launched with
        // -conf=/tmp/rpc.conf (see apps/bitcoin-core & bitcoin-knots
        // manifest.yml) and never reads a datadir bitcoin.conf — if one
        // exists, Bitcoin Core's own safety check refuses to start at all.
        if matches!(package_id, "bitcoin" | "bitcoin-core" | "bitcoin-knots") {
            remove_stale_bitcoin_conf().await?;
        }

        if package_id == "lnd" {
            self.write_lnd_conf(&rpc_user, &rpc_pass).await?;
        }

        // Pre-install: SearXNG settings.yml (required or container exits immediately)
        if package_id == "searxng" {
            let searx_dir = "/var/lib/archipelago/searxng";
            let settings_path = format!("{}/settings.yml", searx_dir);
            if !tokio::fs::try_exists(&settings_path).await.unwrap_or(false) {
                // KEY-05: SearXNG's `server.secret_key` signs that app's own
                // tokens — an app secret, so source named and draw guarded.
                let mut secret = [0u8; 32];
                crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut secret).map_err(
                    |e| {
                        anyhow::anyhow!(
                            "Refusing to write a SearXNG secret_key from degenerate entropy: {e}"
                        )
                    },
                )?;
                let secret_hex = hex::encode(secret);
                let settings = format!(
                    // `search.formats` MUST include json. SearXNG's default is
                    // html-only, so the JSON API answers 403 — and AIUI's web
                    // search is a JSON client (`/aiui/api/web-search` proxies
                    // straight to `/search`). Without this line every AIUI web
                    // search fails on a node whose SearXNG is running
                    // perfectly, which reads as the AI being unable to search
                    // rather than as one missing config key.
                    "use_default_settings: true\ngeneral:\n  instance_name: Archipelago Search\nserver:\n  secret_key: \"{}\"\n  bind_address: \"0.0.0.0\"\n  port: 8080\n  limiter: false\nsearch:\n  formats:\n    - html\n    - json\nui:\n  default_theme: simple\n",
                    secret_hex
                );
                tokio::fs::create_dir_all(searx_dir)
                    .await
                    .context("Failed to create SearXNG config directory")?;
                tokio::fs::write(&settings_path, settings)
                    .await
                    .context("Failed to write SearXNG settings.yml")?;
                info!("Created SearXNG settings.yml");
            }
        }

        // NOW chown data directories to container UID (after all config files are written)
        self.create_data_dirs(package_id, &volumes).await;

        for port in &ports {
            // Drop publishes whose bind address the host can't hold (e.g. the
            // archy-net gateway 10.89.0.1 under rootless podman) — passing
            // them through makes `podman run` itself fail and takes the app
            // down. "ip:host:container" has 2 colons; plain "host:container"
            // has 1 (no IPv6 binds in the legacy string table).
            let bind = if port.matches(':').count() == 2 {
                port.split(':').next().unwrap_or("")
            } else {
                ""
            };
            if !archipelago_container::manifest::host_can_bind_publish_ip(bind) {
                warn!(
                    app = %package_id,
                    publish = %port,
                    "dropping publish: bind address not assignable on this host"
                );
                continue;
            }
            run_args.push("-p");
            run_args.push(port);
        }

        // Volume mounts
        for volume in &volumes {
            run_args.push("-v");
            run_args.push(volume);
        }

        // Environment variables
        for env in &env_vars {
            run_args.push("-e");
            run_args.push(env);
        }

        // Resource limits
        let memory_limit = get_memory_limit(package_id);
        let mem_arg = format!("--memory={}", memory_limit);
        run_args.push(&mem_arg);
        // Bitcoin (and friends) need every core they can get during initial
        // blockchain download — script verification is parallelizable and
        // the limiting factor on most home boxes. --cpus=2 was halving sync
        // speed for 4-8 core machines. ElectrumX likewise scales with cores
        // during its initial reorg/indexing phase.
        let cpu_capped = !matches!(
            package_id,
            "bitcoin"
                | "bitcoin-core"
                | "bitcoin-knots"
                | "electrumx"
                | "electrs"
                | "mempool-electrs"
        );
        if cpu_capped {
            run_args.push("--cpus=2");
        }

        // Uptime Kuma image entrypoint (`extra/entrypoint.sh`) attempts
        // `setpriv --clear-groups` and fails under our rootless + cap-drop
        // defaults. Run the server directly via dumb-init to keep startup
        // stable on production nodes.
        if package_id == "uptime-kuma" {
            run_args.push("--entrypoint=/usr/bin/dumb-init");
        }

        // Health checks
        let health_args = get_health_check_args(package_id, &rpc_pass);
        for arg in &health_args {
            run_args.push(arg);
        }

        // Image — prefer local build over registry
        let effective_image = if has_local_fallback {
            format!("localhost/{}:latest", package_id)
        } else {
            docker_image.to_string()
        };
        run_args.push(&effective_image);

        // Bitcoin-dependent apps (LND, electrs, BTCPay…) exit immediately if
        // bitcoind's RPC isn't answering when they start; the 60s post-start
        // poll then reads that exit as INSTALL CRASH and the whole install
        // fails — the "LND took 5 attempts" failure mode on fresh installs.
        // Gate the container start on the RPC actually responding (IBD is
        // fine — getblockchaininfo answers during sync) with a generous wait,
        // and fail with an actionable message instead of a crash-looping app.
        wait_for_bitcoin_rpc_gate(package_id).await?;

        install_log(&format!(
            "INSTALL RUN: {} — podman run {} (image: {})",
            package_id, container_name, effective_image
        ))
        .await;
        debug!("Running container with args: {:?}", run_args);

        let command_tail = install_command_tail(custom_command.as_deref(), custom_args.as_ref());
        let mut run_output = podman_run_for_install(package_id, &run_args, &command_tail)
            .await
            .context("Failed to run container")?;

        if !run_output.status.success() {
            let stderr = String::from_utf8_lossy(&run_output.stderr).to_string();
            if cleanup_start_conflict(package_id, &stderr).await {
                let _ = tokio::process::Command::new("podman")
                    .args(["rm", "-f", container_name])
                    .output()
                    .await;
                run_output = podman_run_for_install(package_id, &run_args, &command_tail)
                    .await
                    .context("Failed to rerun container")?;
            }
        }

        if !run_output.status.success() {
            let stderr = String::from_utf8_lossy(&run_output.stderr);
            install_log(&format!(
                "INSTALL FAIL: {} — podman run failed: {}",
                package_id, stderr
            ))
            .await;
            // Rollback: remove partially created container
            let _ = tokio::process::Command::new("podman")
                .args(["rm", "-f", container_name])
                .output()
                .await;
            return Err(anyhow::anyhow!("Failed to start container: {}", stderr));
        }

        let container_id = String::from_utf8_lossy(&run_output.stdout)
            .trim()
            .to_string();
        install_log(&format!(
            "INSTALL CREATED: {} — container_id={}",
            package_id,
            &container_id[..12.min(container_id.len())]
        ))
        .await;

        // Phase: StartingContainer — podman run accepted. Next we poll
        // inspect until State.Status == running (up to 60s).
        self.set_install_phase(package_id, InstallPhase::StartingContainer)
            .await;

        // Post-start health verification: wait up to 60s for container to be running
        let mut container_running = false;
        for i in 0..12u32 {
            // After the first poll, flip the UI to WaitingHealthy — the
            // container hasn't come up yet, so the phase label changes
            // from "Starting container" to "Waiting for healthy".
            if i == 1 {
                self.set_install_phase(package_id, InstallPhase::WaitingHealthy)
                    .await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let status = timeout(
                Duration::from_secs(10),
                tokio::process::Command::new("podman")
                    .args(["inspect", container_name, "--format", "{{.State.Status}}"])
                    .output(),
            )
            .await;
            if let Ok(Ok(o)) = status {
                let state = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if state == "running" {
                    container_running = true;
                    break;
                }
                if state == "exited" {
                    // Container crashed immediately — get logs for diagnosis
                    // Keep the container (don't rm) so it shows as "exited" in My Apps
                    // instead of vanishing completely. User can retry or remove manually.
                    let logs = tokio::process::Command::new("podman")
                        .args(["logs", "--tail", "20", container_name])
                        .output()
                        .await;
                    let log_output = logs
                        .map(|o| {
                            let stdout = String::from_utf8_lossy(&o.stdout);
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            format!("{}{}", stdout, stderr)
                        })
                        .unwrap_or_default();
                    install_log(&format!(
                        "INSTALL CRASH: {} — container exited (kept for visibility). Logs:\n{}",
                        package_id,
                        &log_output.chars().take(1000).collect::<String>()
                    ))
                    .await;
                    return Err(anyhow::anyhow!(
                        "Container {} exited immediately after start. Logs: {}",
                        container_name,
                        log_output.chars().take(500).collect::<String>()
                    ));
                }
            } else {
                warn!("podman inspect {} timed out during install", container_name);
            }
            if i == 11 {
                warn!(
                    "Container {} not running after 60s — install may have failed",
                    container_name
                );
            }
        }

        if !container_running {
            install_log(&format!(
                "INSTALL TIMEOUT: {} — not running after 60s",
                package_id
            ))
            .await;
            return Err(anyhow::anyhow!(
                "Container {} did not reach running state within 60s. Check logs with: podman logs {}",
                container_name, container_name
            ));
        }

        // Phase: PostInstall — container is up and running. Now any
        // app-specific post-install (chain init, wallet setup, waiting
        // for a first block). Varies by app; some are no-ops.
        self.set_install_phase(package_id, InstallPhase::PostInstall)
            .await;

        // Post-install hooks — await completion before returning success
        self.run_post_install_hooks(package_id).await;

        if package_id == "nextcloud" {
            repair_nextcloud_permissions().await;
        }

        ensure_host_port_listener(package_id, container_name, &ports).await?;

        install_log(&format!(
            "INSTALL OK: {} (container: {})",
            package_id,
            &container_id[..12.min(container_id.len())]
        ))
        .await;

        Ok(serde_json::json!({
            "success": true,
            "package_id": package_id,
            "container_id": container_id,
            "message": format!("Package {} installed and started", package_id)
        }))
    }

    /// Package ids currently mid-install/update per the state manager — used
    /// by the dependency gate so an app whose dependency is still pulling its
    /// image (no container yet) waits instead of failing fast.
    async fn packages_currently_installing(&self) -> Vec<String> {
        let (data, _) = self.state_manager.get_snapshot().await;
        data.package_data
            .iter()
            .filter(|(_, entry)| {
                matches!(
                    entry.state,
                    crate::data_model::PackageState::Installing
                        | crate::data_model::PackageState::Updating
                )
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    async fn running_deps_for_install(&self, package_id: &str) -> Result<RunningDeps> {
        let (data, _) = self.state_manager.get_snapshot().await;
        let cached = detect_running_deps_from_package_data(&data.package_data);
        if dependency_cache_satisfies(package_id, &cached) {
            Ok(cached)
        } else {
            detect_running_deps().await
        }
    }

    /// Bounded dependency gate for installs: passes immediately when deps are
    /// running, fails fast (with the phantom-tile marker) when a dependency
    /// isn't installed at all, and otherwise waits up to
    /// `DEP_WAIT_MAX_ATTEMPTS × DEP_WAIT_INTERVAL` for an installed-but-
    /// starting dependency, surfacing "Waiting for X to start…" on the card.
    pub(super) async fn gate_install_deps(&self, package_id: &str) -> Result<RunningDeps> {
        wait_for_install_deps(
            package_id,
            || async {
                Ok(DepProbe {
                    running: self.running_deps_for_install(package_id).await?,
                    existing: detect_existing_containers().await,
                    installing: self.packages_currently_installing().await,
                })
            },
            |msg| async move { self.set_install_message(package_id, &msg).await },
            DEP_WAIT_MAX_ATTEMPTS,
            DEP_WAIT_INTERVAL,
        )
        .await
    }

    // -- Private helpers for install --

    /// Pull the image from a registry or verify a local image exists.
    /// Returns `true` if a local fallback image was found (registry pull skipped).
    async fn pull_or_verify_image(&self, package_id: &str, docker_image: &str) -> Result<bool> {
        let is_local_image = docker_image.starts_with("localhost/");
        let has_local_fallback = if !is_local_image {
            let local_tag = format!("localhost/{}:latest", package_id);
            local_podman_image_exists(&local_tag).await.unwrap_or(false)
        } else {
            false
        };

        if !is_local_image && !has_local_fallback {
            self.pull_image_with_progress(package_id, docker_image)
                .await?;
        } else if has_local_fallback {
            debug!(
                "Using local build for {} (skipping registry pull)",
                package_id
            );
        } else {
            // Local image — verify it exists
            if !local_podman_image_exists(docker_image)
                .await
                .context("Failed to check local image")?
            {
                return Err(anyhow::anyhow!(
                    "Local image {} not found. Build the image first \
                     or ensure the registry is reachable.",
                    docker_image
                ));
            }
            debug!("Using local image: {}", docker_image);
        }

        Ok(has_local_fallback)
    }

    /// Pull image once through the configured registry list. Each registry URL
    /// already has a bounded timeout, so retrying the full list can leave the UI
    /// in Installing for close to an hour when a large image is unavailable or
    /// a registry stalls.
    async fn pull_image_with_progress(&self, package_id: &str, docker_image: &str) -> Result<()> {
        if let Err(e) = self.do_pull_image(package_id, docker_image).await {
            self.clear_install_progress(package_id).await;
            return Err(e.context(format!("Failed to pull {}", docker_image)));
        }
        Ok(())
    }

    /// Pull one image URL with live progress streamed through
    /// `update_install_progress`. Returns Ok(true) on a successful pull,
    /// Ok(false) on transient failure (so the caller can try the next
    /// mirror), Err only for unrecoverable setup errors.
    async fn pull_one_url_with_progress(
        &self,
        url: &str,
        tls_verify: bool,
        package_id: &str,
        user_tmp: &str,
    ) -> Result<bool> {
        let mut pull_args = vec!["pull".to_string(), url.to_string()];
        if !tls_verify {
            pull_args.push("--tls-verify=false".to_string());
        }
        let mut child = tokio::process::Command::new("podman")
            .args(&pull_args)
            .env("TMPDIR", user_tmp)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to start image pull")?;

        // Stall-aware budget instead of a hard wall clock. The old fixed
        // 300s cap killed slow-but-progressing pulls (LND on fresh-node
        // WiFi routinely needs longer), which surfaced as "first install
        // fails, second succeeds" once the layer cache was warm. A pull is
        // only killed when NOTHING is observably happening — no stderr
        // output and no byte growth in podman's staging dir — for
        // PULL_STALL_TIMEOUT_SECS, or at a generous absolute ceiling.
        // Dead registries still fail fast: no bytes ever land, so the
        // stall window (3 min) is the effective bound.
        const PULL_STALL_TIMEOUT_SECS: u64 = 180;
        const PULL_URL_MAX_SECS: u64 = 1800;
        const PULL_POLL_INTERVAL_SECS: u64 = 5;

        let started = std::time::Instant::now();
        // Seconds-since-start of the last stderr line, updated by the
        // reader task. u64::MAX sentinel = no line seen yet (treated as
        // activity at t=0 so the stall window starts at spawn).
        let last_line_at = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let pkg_id = package_id.to_string();
            let state_mgr = self.state_manager.clone();
            let line_clock = std::sync::Arc::clone(&last_line_at);
            let started_reader = started;
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    line_clock.store(
                        started_reader.elapsed().as_secs(),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    if let Some((downloaded, total)) = parse_pull_progress(&line) {
                        Self::update_install_progress(&state_mgr, &pkg_id, downloaded, total).await;
                    }
                }
            });
        }

        let mut last_staged_bytes = dir_size_bytes(user_tmp);
        let mut last_staged_change = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status.success()),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Image pull process error on {}: {}", url, e);
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Ok(false);
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(PULL_POLL_INTERVAL_SECS)).await;

            if started.elapsed() > std::time::Duration::from_secs(PULL_URL_MAX_SECS) {
                tracing::warn!(
                    "Image pull exceeded absolute {}s ceiling: {}",
                    PULL_URL_MAX_SECS,
                    url
                );
                let _ = child.kill().await;
                let _ = child.wait().await; // reap zombie
                return Ok(false);
            }

            // Activity signal 1: stderr output from podman.
            let line_age = started
                .elapsed()
                .as_secs()
                .saturating_sub(last_line_at.load(std::sync::atomic::Ordering::Relaxed));
            // Activity signal 2: podman stages layer downloads in TMPDIR
            // (user_tmp) — any size change there means bytes are moving.
            let staged = dir_size_bytes(user_tmp);
            if staged != last_staged_bytes {
                last_staged_bytes = staged;
                last_staged_change = std::time::Instant::now();
            }

            let stalled = line_age > PULL_STALL_TIMEOUT_SECS
                && last_staged_change.elapsed()
                    > std::time::Duration::from_secs(PULL_STALL_TIMEOUT_SECS);
            if stalled {
                tracing::warn!(
                    "Image pull stalled ({}s with no output and no staged bytes): {}",
                    PULL_STALL_TIMEOUT_SECS,
                    url
                );
                let _ = child.kill().await;
                let _ = child.wait().await; // reap zombie
                return Ok(false);
            }
        }
    }

    /// Pull a container image, trying each configured registry in
    /// priority order and streaming progress during every attempt. The
    /// primary is tried first; if it doesn't have the image (or 404's),
    /// the next mirror is tried — with its own progress streaming, so
    /// the UI doesn't freeze at 0% after a primary miss. On success the
    /// image is tagged under `docker_image` so downstream commands
    /// (images -q, run -d, etc.) can find it by its canonical name.
    async fn do_pull_image(&self, package_id: &str, docker_image: &str) -> Result<()> {
        debug!("Pulling image: {}", docker_image);
        self.set_install_progress(package_id, 0, 0).await;

        // Set TMPDIR to user-writable location — rootless podman's user namespace
        // makes /var/tmp read-only, which causes `podman pull` to fail with
        // "mkdir /var/tmp/container_images_storage...: read-only file system"
        let user_tmp = format!(
            "{}/.local/share/containers/tmp",
            std::env::var("HOME").unwrap_or_else(|_| "/home/archipelago".to_string())
        );
        let _ = std::fs::create_dir_all(&user_tmp);

        // Build the ordered candidate list: every enabled registry
        // (highest priority first), each rewriting the image URL to its
        // own origin. Deduplicate — two registries that happen to share
        // a URL should only be tried once.
        let config = crate::container::registry::load_registries(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let mut tried: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut candidates: Vec<(String, bool)> = Vec::new();
        for reg in config.active_registries() {
            let url = config.rewrite_image(docker_image, reg);
            if tried.insert(url.clone()) {
                candidates.push((url, reg.tls_verify));
            }
        }
        // Always include the literal URL as a last-resort candidate —
        // internal mirrors may not host every third-party upstream image
        // (e.g. docker.io/bitcoin/bitcoin:28.4), and we don't want
        // "app not mirrored" to masquerade as a generic pull failure.
        if tried.insert(docker_image.to_string()) {
            candidates.push((docker_image.to_string(), true));
        }

        // Walk candidates, streaming progress for each attempt.
        let mut pulled_url: Option<String> = None;
        let attempts = candidates.len();
        for (i, (url, tls_verify)) in candidates.iter().enumerate() {
            if url != docker_image {
                debug!("Attempt {}/{}: {}", i + 1, attempts, url);
            } else {
                debug!("Attempt {}/{}: {} (literal)", i + 1, attempts, url);
            }
            // Reset progress at the top of each attempt so the UI reflects
            // the fresh pull instead of showing stale bytes from a prior
            // partial attempt.
            self.set_install_progress(package_id, 0, 0).await;
            let started = std::time::Instant::now();
            match self
                .pull_one_url_with_progress(url, *tls_verify, package_id, &user_tmp)
                .await?
            {
                true => {
                    tracing::info!("Pulled {} from {}", docker_image, url);
                    pulled_url = Some(url.clone());
                    break;
                }
                false => {
                    // A fast failure (DNS miss / TCP reset while a fresh
                    // node's network is still warming up) is worth one quick
                    // retry — observed on first-boot installs where the
                    // second click succeeded. A slow failure already spent
                    // the 300s budget; retrying it just doubles the wait.
                    if started.elapsed() < std::time::Duration::from_secs(30) {
                        tracing::info!("Fast pull failure on {}, retrying once in 5s", url);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        self.set_install_progress(package_id, 0, 0).await;
                        if self
                            .pull_one_url_with_progress(url, *tls_verify, package_id, &user_tmp)
                            .await?
                        {
                            tracing::info!("Pulled {} from {} (retry)", docker_image, url);
                            pulled_url = Some(url.clone());
                            break;
                        }
                    }
                    tracing::debug!(
                        "Pull attempt {}/{} failed for {}, trying next mirror",
                        i + 1,
                        attempts,
                        url
                    );
                    continue;
                }
            }
        }

        let Some(pulled_url) = pulled_url else {
            return Err(anyhow::anyhow!(
                "Image pull failed from all {} configured registries for {}",
                attempts,
                docker_image
            ));
        };

        // Tag under the original docker_image reference if the successful
        // pull came from a rewritten URL — downstream code (images -q,
        // run -d docker_image, etc.) needs to find it by its canonical
        // name regardless of which mirror actually served the bytes.
        if pulled_url != docker_image {
            let _ = tokio::process::Command::new("podman")
                .args(["tag", &pulled_url, docker_image])
                .output()
                .await;
        }

        // Verify image exists locally after pull.
        if !local_podman_image_exists(docker_image)
            .await
            .context("Failed to verify pulled image")?
        {
            return Err(anyhow::anyhow!(
                "Image {} not found locally after pull",
                docker_image
            ));
        }

        self.set_install_progress(package_id, 100, 100).await;
        Ok(())
    }

    /// Create data directories for volume mounts under /var/lib/archipelago/.
    /// Get the mapped host UID for a container's internal UID.
    /// Rootless podman UID maps commonly look like:
    ///   container 0 -> host real uid (e.g. 1000)
    ///   container 1.. -> host subuid range starting at 100000
    /// So for uid>=1, host_uid = 99999 + container_uid.
    fn mapped_uid(package_id: &str) -> u32 {
        let container_uid = match package_id {
            "bitcoin-knots" | "bitcoin" | "bitcoin-core" => 101,
            "grafana" => 472,
            "lnd" => 1000,
            "mariadb" | "mysql" | "mysql-mempool" | "archy-mempool-db" => 999,
            "postgres" | "immich-postgres" | "nextcloud-db" => 70,
            // Current BTCPay Postgres image runs as uid 999 inside the
            // container, so its rootless host-mapped uid is 100998.
            "btcpay-postgres" | "archy-btcpay-db" => 999,
            "electrumx" | "electrs" => 0,
            _ => 0, // Most containers run as root (UID 0)
        };
        if container_uid == 0 {
            // Archipelago daemon runs as rootless user (typically uid 1000).
            // Container uid 0 maps to that real host uid.
            1000
        } else {
            99999 + container_uid
        }
    }

    async fn create_data_dirs(&self, package_id: &str, volumes: &[String]) {
        let uid = Self::mapped_uid(package_id);
        let uid_str = format!("{}:{}", uid, uid);

        for volume in volumes {
            if let Some(host_path) = volume.split(':').next() {
                if host_path.starts_with("/var/lib/archipelago/") {
                    debug!("Creating directory: {} (owner: {})", host_path, uid_str);

                    // Create directory directly (service has ReadWritePaths access).
                    if let Err(e) = std::fs::create_dir_all(host_path) {
                        tracing::warn!("Failed to create directory {}: {}", host_path, e);
                    }

                    // Set ownership to the mapped UID for rootless podman.
                    // Try sudo chown first, then fall back to podman unshare
                    // for subuid-mapped UIDs only.
                    let host_uid = format!("{}:{}", uid, uid);
                    let sudo_result = tokio::process::Command::new("sudo")
                        .args(["chown", "-R", &host_uid, host_path])
                        .output()
                        .await;
                    let sudo_ok = sudo_result.as_ref().is_ok_and(|o| o.status.success());

                    if !sudo_ok {
                        if uid >= 100000 {
                            let container_uid = uid - 100000;
                            let container_uid_str = format!("{}:{}", container_uid, container_uid);
                            let chown_result = tokio::process::Command::new("podman")
                                .args(["unshare", "chown", "-R", &container_uid_str, host_path])
                                .output()
                                .await;
                            match chown_result {
                                Ok(out) if !out.status.success() => {
                                    tracing::warn!(
                                        "chown failed for {} (both sudo and podman unshare)",
                                        host_path,
                                    );
                                }
                                Err(e) => tracing::warn!("Failed to chown {}: {}", host_path, e),
                                _ => {}
                            }
                        } else {
                            tracing::warn!(
                                "chown fallback skipped for {}: host uid {} has no subuid mapping",
                                host_path,
                                uid
                            );
                        }
                    }
                }
            }
        }
    }

    // write_bitcoin_conf removed: bitcoind is launched with -conf=/tmp/rpc.conf
    // (see apps/bitcoin-core & bitcoin-knots manifest.yml, commit a597c1d9)
    // and never reads a datadir bitcoin.conf. Writing one here created a
    // fatal "-conf vs default bitcoin.conf" conflict on every subsequent
    // start (Bitcoin Core's own datadir-conflict safety check). See
    // `remove_stale_bitcoin_conf` below, which replaces both this and
    // `ensure_bitcoin_rpc_config`.

    /// Write LND config file with Bitcoin RPC credentials.
    async fn write_lnd_conf(&self, rpc_user: &str, rpc_pass: &str) -> Result<()> {
        let lnd_dir = "/var/lib/archipelago/lnd";
        let conf_path = format!("{}/lnd.conf", lnd_dir);

        // Don't overwrite existing config (user may have customized it)
        if tokio::fs::try_exists(&conf_path).await.unwrap_or(false) {
            info!("lnd.conf already exists, skipping write");
            return Ok(());
        }

        let lnd_conf = format!(
            "\
[Application Options]\n\
listen=0.0.0.0:9735\n\
rpclisten=0.0.0.0:10009\n\
restlisten=0.0.0.0:8080\n\
debuglevel=info\n\
noseedbackup=true\n\
tlsextraip=0.0.0.0\n\
tlsextradomain=lnd\n\
tor.active=true\n\
tor.socks=host.containers.internal:9050\n\
tor.streamisolation=true\n\
\n\
[Bitcoin]\n\
bitcoin.mainnet=true\n\
bitcoin.node=bitcoind\n\
\n\
[Bitcoind]\n\
bitcoind.rpchost=host.containers.internal:8332\n\
bitcoind.rpcuser={user}\n\
bitcoind.rpcpass={pass}\n\
bitcoind.rpcpolling=true\n\
bitcoind.estimatemode=ECONOMICAL\n\
\n\
[autopilot]\n\
autopilot.active=false\n",
            user = rpc_user,
            pass = rpc_pass,
        );
        tokio::fs::create_dir_all(lnd_dir)
            .await
            .context("Failed to create LND data directory")?;
        tokio::fs::write(&conf_path, lnd_conf)
            .await
            .context("Failed to write lnd.conf")?;
        info!("Created lnd.conf with Bitcoin RPC credentials");
        Ok(())
    }

    /// Run post-install hooks (Nextcloud trusted domains, Bitcoin UI container).
    /// Critical hooks (credential setup, config) are awaited; UI container builds are background.
    async fn run_post_install_hooks(&self, package_id: &str) {
        if matches!(package_id, "homeassistant" | "home-assistant")
            && super::pine_ha::pine_engines_installed().await
        {
            // Pine was installed first: wire HA to its Wyoming engines now,
            // then restart so the seeded storage is what HA loads.
            if super::pine_ha::seed_home_assistant_pine_defaults().await {
                super::pine_ha::restart_home_assistant_if_running().await;
            }
        }
        if package_id == "filebrowser" {
            // Generate a random password (32 bytes, hex-encoded)
            let mut buf = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut buf);
            let password = hex::encode(buf);

            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Failed to create HTTP client for FileBrowser hook: {}", e);
                    return;
                }
            };

            // Retry loop: FileBrowser may take time to initialize its SQLite database
            let mut password_changed = false;
            for attempt in 0..6u32 {
                let delay = if attempt == 0 { 5 } else { 10 };
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;

                // Try to log in with default credentials
                let login_res = client
                    .post("http://127.0.0.1:8083/api/login")
                    .json(&serde_json::json!({"username": "admin", "password": "admin"}))
                    .send()
                    .await;

                let token = match login_res {
                    Ok(resp) if resp.status().is_success() => match resp.text().await {
                        Ok(t) => t.trim_matches('"').to_string(),
                        Err(_) => continue,
                    },
                    _ => {
                        debug!("FileBrowser not ready (attempt {}/6)", attempt + 1);
                        continue;
                    }
                };

                // Change admin password
                let change_res = client
                    .put("http://127.0.0.1:8083/api/users/1")
                    .header("X-Auth", &token)
                    .json(&serde_json::json!({"password": password}))
                    .send()
                    .await;

                match change_res {
                    Ok(resp) if resp.status().is_success() => {
                        let secret_dir = "/var/lib/archipelago/secrets/filebrowser";
                        if let Err(e) = tokio::fs::create_dir_all(secret_dir).await {
                            tracing::warn!("Failed to create filebrowser secrets dir: {}", e);
                        }
                        let pw_path = format!("{}/password", secret_dir);
                        if let Err(e) = tokio::fs::write(&pw_path, &password).await {
                            tracing::warn!("Failed to write filebrowser password: {}", e);
                        }
                        // Set restrictive permissions on the password file
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = std::fs::set_permissions(
                                &pw_path,
                                std::fs::Permissions::from_mode(0o600),
                            );
                        }
                        info!("FileBrowser admin password secured (default credentials replaced)");
                        password_changed = true;
                        break;
                    }
                    _ => continue,
                }
            }
            if !password_changed {
                tracing::warn!(
                    "FileBrowser password could not be changed after 6 attempts — \
                     default credentials (admin/admin) remain active"
                );
            }
        }

        // Auto-configure Tor hidden service for protocol services (LND, ElectrumX, Bitcoin)
        {
            use crate::api::rpc::tor::{
                is_protocol_service, known_service_port, load_services_config, regenerate_torrc,
                restart_tor, save_services_config, sync_single_hostname, wait_for_hostname,
                TorServiceEntry,
            };

            let tor_port = known_service_port(package_id);
            if tor_port > 0 {
                let config_dir = self.config.data_dir.join("tor-config");
                let mut config = load_services_config(&config_dir).await;
                let already_exists = config.services.iter().any(|s| s.name == package_id);

                if !already_exists {
                    let is_proto = is_protocol_service(package_id);
                    config.services.push(TorServiceEntry {
                        name: package_id.to_string(),
                        local_port: tor_port,
                        remote_port: None,
                        unauthenticated: is_proto,
                        enabled: true,
                    });
                    if let Err(e) = save_services_config(&config_dir, &config).await {
                        tracing::warn!("Failed to save Tor config for {}: {}", package_id, e);
                    } else if let Err(e) = regenerate_torrc(&config).await {
                        tracing::warn!("Failed to regenerate torrc for {}: {}", package_id, e);
                    } else if let Err(e) = restart_tor().await {
                        tracing::warn!("Failed to restart Tor for {}: {}", package_id, e);
                    } else {
                        let onion = wait_for_hostname(package_id, 30).await;
                        if let Some(ref addr) = onion {
                            sync_single_hostname(package_id, addr).await;
                            info!("Tor hidden service created for {} → {}", package_id, addr);
                        } else {
                            info!(
                                "Tor hidden service created for {} (hostname pending)",
                                package_id
                            );
                        }
                    }
                }
            }
        }

        // IndeeHub: inject the current consent-gated provider and make it work
        // in both the dashboard frame and a direct browser tab.
        if package_id == "indeedhub" {
            patch_indeedhub_nostr_provider().await;
        }

        // Gitea owns its public URL and security settings in app.ini, including
        // values chosen in its first-run setup. Do not rewrite operator values
        // or claim success from best-effort grep/sed commands. The app gate
        // fronts its declared HTTP port and handles frame headers separately.

        if package_id == "nextcloud" {
            let host_ip = &self.config.host_ip;
            // Wait for Nextcloud to finish first-run initialization
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            for domain_idx in 1..=2u8 {
                let value = if domain_idx == 1 {
                    host_ip.as_str()
                } else {
                    "localhost"
                };
                let _ = tokio::process::Command::new("podman")
                    .args([
                        "exec",
                        "-u",
                        "33",
                        "nextcloud",
                        "php",
                        "occ",
                        "config:system:set",
                        "trusted_domains",
                        &domain_idx.to_string(),
                        "--value",
                        value,
                    ])
                    .output()
                    .await;
            }
            info!("Nextcloud trusted domains configured for {}", host_ip);
        }

        // Companion UIs (archy-bitcoin-ui, archy-lnd-ui, archy-electrs-ui)
        // are now Quadlet-managed: install_for writes ~/.config/containers/
        // systemd/<name>.container, daemon-reloads, and starts the generated
        // .service. systemd owns supervision from there — companions survive
        // archipelago crashes, restarts, and OOM kills. Per-node config
        // (e.g. bitcoin-ui's nginx.conf with the live RPC auth) is rendered
        // by each spec's pre_start hook and bind-mounted read-only.
        for (name, err) in crate::container::companion::install_for(package_id).await {
            install_log(&format!("COMPANION FAIL: {name} — {err:#}")).await;
        }
    }

    // ── Registry management ──

    pub(in crate::api::rpc) async fn handle_registry_list(&self) -> Result<serde_json::Value> {
        let config = crate::container::registry::load_registries(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "registries": config.registries }))
    }

    pub(in crate::api::rpc) async fn handle_registry_add(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or(url);
        let tls_verify = params
            .get("tls_verify")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let priority = params
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32;

        if url.is_empty() {
            return Err(anyhow::anyhow!("Registry URL cannot be empty"));
        }

        let mut config = crate::container::registry::load_registries(&self.config.data_dir).await?;

        if config.registries.iter().any(|r| r.url == url) {
            return Err(anyhow::anyhow!("Registry '{}' already exists", url));
        }

        config
            .registries
            .push(crate::container::registry::Registry {
                url: url.to_string(),
                name: name.to_string(),
                tls_verify,
                enabled: true,
                priority,
            });

        crate::container::registry::save_registries(&self.config.data_dir, &config).await?;
        Ok(serde_json::json!({ "registries": config.registries, "added": url }))
    }

    pub(in crate::api::rpc) async fn handle_registry_remove(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;

        let mut config = crate::container::registry::load_registries(&self.config.data_dir).await?;
        let before = config.registries.len();
        config.registries.retain(|r| r.url != url);

        if config.registries.len() == before {
            return Err(anyhow::anyhow!("Registry '{}' not found", url));
        }

        crate::container::registry::save_registries(&self.config.data_dir, &config).await?;
        Ok(serde_json::json!({ "registries": config.registries, "removed": url }))
    }

    /// Promote a registry to primary by resetting priorities — the named
    /// URL becomes priority 0, every other enabled registry is bumped up
    /// by 10. Order is stable (ties broken by original priority).
    pub(in crate::api::rpc) async fn handle_registry_set_primary(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;

        let mut config = crate::container::registry::load_registries(&self.config.data_dir).await?;
        if !config.registries.iter().any(|r| r.url == url) {
            return Err(anyhow::anyhow!("Registry '{}' not found", url));
        }

        // Reassign priorities: target = 0, everyone else = 10, 20, 30…
        // in their existing priority order.
        let target_url = url.to_string();
        config
            .registries
            .sort_by_key(|r| (r.url != target_url, r.priority));
        for (i, r) in config.registries.iter_mut().enumerate() {
            r.priority = if r.url == target_url {
                0
            } else {
                (i as u32) * 10
            };
        }

        crate::container::registry::save_registries(&self.config.data_dir, &config).await?;
        Ok(serde_json::json!({ "registries": config.registries, "primary": url }))
    }

    pub(in crate::api::rpc) async fn handle_registry_test(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing url"))?;
        let tls_verify = params
            .get("tls_verify")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Registries are configured as `host[:port]/namespace` (for
        // example `source.archipelago-foundation.org/lfg2025`), but the Docker V2
        // registry API lives at `/v2/` on the ROOT of the host — NOT
        // under the namespace. Strip the namespace before appending
        // `/v2/` so the reachability probe hits the correct URL.
        let host = url.split('/').next().unwrap_or(url);
        let test_url = if tls_verify {
            format!("https://{}/v2/", host)
        } else {
            format!("http://{}/v2/", host)
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .danger_accept_invalid_certs(!tls_verify)
            .build()
            .unwrap_or_default();

        match client.get(&test_url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                // Accept any "the server responded to HTTP" code as
                // reachable — 200 (open registry), 401 (auth required),
                // or 405 (server rejects GET but the endpoint exists)
                // all mean the registry daemon is alive on the host.
                let reachable = status == 200 || status == 401 || status == 405;
                Ok(serde_json::json!({
                    "url": url,
                    "reachable": reachable,
                    "status": status,
                }))
            }
            Err(e) => Ok(serde_json::json!({
                "url": url,
                "reachable": false,
                "error": e.to_string(),
            })),
        }
    }

    pub(in crate::api::rpc) async fn handle_filebrowser_token(&self) -> Result<serde_json::Value> {
        let secret_path = "/var/lib/archipelago/secrets/filebrowser/password";
        let password = tokio::fs::read_to_string(secret_path)
            .await
            .unwrap_or_else(|_| "admin".to_string());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let resp = client
            .post("http://127.0.0.1:8083/api/login")
            .json(&serde_json::json!({"username": "admin", "password": password}))
            .send()
            .await
            .context("Failed to connect to FileBrowser")?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "FileBrowser login failed ({})",
                resp.status()
            ));
        }

        let token = resp.text().await.unwrap_or_default();
        let token = token.trim_matches('"');

        Ok(serde_json::json!({ "token": token }))
    }

    pub(in crate::api::rpc) async fn handle_package_credentials(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let app_id = params
            .as_ref()
            .and_then(|p| p.get("app_id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        super::validation::validate_app_id(app_id)?;

        if app_id == "filebrowser" {
            let password =
                tokio::fs::read_to_string("/var/lib/archipelago/secrets/filebrowser/password")
                    .await
                    .map(|p| p.trim().to_string())
                    .unwrap_or_else(|_| "admin".to_string());
            return Ok(serde_json::json!({
                "title": "File Browser credentials",
                "description": "Use these credentials when File Browser asks you to sign in.",
                "credentials": [
                    { "label": "Username", "value": "admin" },
                    { "label": "Password", "value": password, "sensitive": true }
                ]
            }));
        }

        if app_id == "photoprism" {
            return Ok(serde_json::json!({
                "title": "PhotoPrism credentials",
                "description": "Use these credentials when PhotoPrism asks you to sign in.",
                "credentials": [
                    { "label": "Username", "value": "admin" },
                    { "label": "Password", "value": "archipelago", "sensitive": true }
                ]
            }));
        }

        // Portainer ≥2.21 no longer lets whoever loads the page first claim the
        // admin account: on a fresh install it mints a one-time setup token and
        // prints it to the SERVER LOGS, expecting the operator to go digging.
        // On an appliance that is hostile UX — "check the Portainer server
        // logs" is exactly the dead end users cannot follow. The token is the
        // only thing standing between the user and their own app, so surface
        // it in the same launch interstitial as the login credentials: extract
        // it from the container logs and hand it over with a copy button.
        // Once setup completes Portainer invalidates the token, and a container
        // recreate (any update) drops the log line entirely — so absence of the
        // line naturally makes the card disappear and no stale token lingers.
        if app_id == "portainer" {
            if let Some(token) = portainer_setup_token(self).await {
                return Ok(serde_json::json!({
                    "title": "Portainer first-run token",
                    "description": "New Portainer versions protect the first launch with a one-time setup token instead of letting anyone on the network claim the admin account. Paste this token into Portainer's setup screen to create your administrator login. It is only valid until setup finishes — if you already created your admin account, ignore this.",
                    "credentials": [
                        { "label": "Setup token", "value": token, "sensitive": true }
                    ]
                }));
            }
        }

        Ok(serde_json::json!({ "credentials": [] }))
    }
}

/// Extract Portainer's first-run `setup_token=…` from the live container's
/// recent logs. `None` when the line is absent (setup already done, or an
/// older Portainer without the token flow).
async fn portainer_setup_token(rpc: &RpcHandler) -> Option<String> {
    let logs = rpc.get_container_logs_value("portainer", 300).await.ok()?;
    let lines = logs.as_array()?;
    let lines: Vec<&str> = lines.iter().filter_map(|l| l.as_str()).collect();
    parse_setup_token(&lines)
}

/// Pure log-line scan: the token is 64 hex chars after `setup_token=`.
/// Sear newest-first so the most recent mint wins.
fn parse_setup_token(lines: &[&str]) -> Option<String> {
    for line in lines.iter().rev() {
        let Some(idx) = line.find("setup_token=") else {
            continue;
        };
        let tail = &line[idx + "setup_token=".len()..];
        let token: String = tail.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if token.len() == 64 {
            return Some(token);
        }
    }
    None
}

async fn cleanup_stale_package_ports(package_id: &str) {
    // Never kill by port: another app or the management gate may own it.
    crate::container::ghost_reaper::reap_for_app(package_id).await;
}

fn install_command_tail(
    custom_cmd: Option<&str>,
    custom_args: Option<&Vec<String>>,
) -> Vec<String> {
    if let Some(cmd) = custom_cmd {
        vec![cmd.to_string()]
    } else if let Some(args) = custom_args {
        args.clone()
    } else {
        Vec::new()
    }
}

async fn podman_run_for_install(
    package_id: &str,
    run_args: &[&str],
    command_tail: &[String],
) -> Result<std::process::Output> {
    if should_scope_podman_run(package_id) {
        match podman_create_then_scoped_start(package_id, run_args, command_tail).await {
            Ok(output) => return Ok(output),
            Err(err) => {
                tracing::warn!(package_id, error = %err, "scoped podman create/start failed; falling back to direct podman run");
            }
        }
    }

    let mut cmd = tokio::process::Command::new("podman");
    cmd.args(run_args);
    cmd.args(command_tail);
    cmd.output().await.context("Failed to run podman")
}

async fn podman_create_then_scoped_start(
    package_id: &str,
    run_args: &[&str],
    command_tail: &[String],
) -> Result<std::process::Output> {
    let container_name = run_args
        .windows(2)
        .find_map(|pair| (pair[0] == "--name").then_some(pair[1]))
        .unwrap_or(package_id);
    let mut create_args = Vec::with_capacity(run_args.len() + command_tail.len());
    for (idx, arg) in run_args.iter().enumerate() {
        if idx == 0 && *arg == "run" {
            create_args.push("create".to_string());
        } else if *arg != "-d" {
            create_args.push((*arg).to_string());
        }
    }
    create_args.extend(command_tail.iter().cloned());

    let mut create = tokio::process::Command::new("podman");
    create.args(&create_args);
    let create_output = create
        .output()
        .await
        .context("Failed to run podman create")?;
    if !create_output.status.success() {
        return Ok(create_output);
    }

    let mut scoped_start = tokio::process::Command::new("systemd-run");
    scoped_start.args([
        "--user",
        "--scope",
        "--quiet",
        "--collect",
        "podman",
        "start",
        container_name,
    ]);
    match scoped_start.output().await {
        Ok(output) if output.status.success() => Ok(create_output),
        Ok(output) => {
            tracing::warn!(
                package_id,
                container = container_name,
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "scoped podman start after create failed; trying direct podman start"
            );
            let mut direct_start = tokio::process::Command::new("podman");
            direct_start.args(["start", container_name]);
            let direct_output = direct_start
                .output()
                .await
                .context("Failed to run fallback podman start")?;
            if direct_output.status.success() {
                Ok(create_output)
            } else {
                Ok(direct_output)
            }
        }
        Err(err) => Err(err).context("Failed to run scoped podman start"),
    }
}

fn should_scope_podman_run(package_id: &str) -> bool {
    matches!(
        package_id,
        "botfights"
            | "filebrowser"
            | "gitea"
            | "grafana"
            | "homeassistant"
            | "home-assistant"
            | "jellyfin"
            | "nginx-proxy-manager"
            | "nostr-rs-relay"
            | "photoprism"
            | "portainer"
            | "searxng"
            | "uptime-kuma"
            | "vaultwarden"
    )
}

async fn cleanup_start_conflict(package_id: &str, stderr: &str) -> bool {
    if stderr.contains("name is already in use") || stderr.contains("name \"") {
        return true;
    }

    if stderr.contains("pasta failed") || stderr.contains("address already in use") {
        crate::container::ghost_reaper::reap_for_app(package_id).await;
        return true;
    }
    false
}

async fn repair_nextcloud_permissions() {
    let script = "chmod 755 /var/www/html /var/www/html/config /var/www/html/data 2>/dev/null || true; chmod 644 /var/www/html/.htaccess /var/www/html/index.php /var/www/html/status.php /var/www/html/config/.htaccess 2>/dev/null || true; chmod 640 /var/www/html/config/config.php 2>/dev/null || true";
    let output = tokio::process::Command::new("podman")
        .args(["exec", "nextcloud", "sh", "-lc", script])
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!("Nextcloud permission repair failed: {}", stderr.trim());
        }
        Err(err) => tracing::warn!("Failed to run Nextcloud permission repair: {}", err),
    }
}

async fn ensure_host_port_listener(
    package_id: &str,
    container_name: &str,
    runtime_ports: &[String],
) -> Result<()> {
    let mut port = runtime_ports
        .first()
        .and_then(|p| p.split(':').next())
        .and_then(|p| p.parse::<u16>().ok());
    if port.is_none() {
        port = published_host_port(container_name).await;
    }
    let Some(port) = port.or_else(|| required_host_port(package_id)) else {
        return Ok(());
    };

    if wait_for_host_port(package_id, port, 10).await {
        return Ok(());
    }

    install_log(&format!(
        "INSTALL REPAIR: {} — host port {} missing after start; restarting container",
        package_id, port
    ))
    .await;
    cleanup_stale_package_ports(package_id).await;

    let output = tokio::process::Command::new("podman")
        .args(["restart", container_name])
        .output()
        .await
        .context("failed to restart container after missing host port")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Container {} host port {} was not listening and restart failed: {}",
            container_name,
            port,
            stderr.trim()
        ));
    }

    if wait_for_host_port(package_id, port, 60).await {
        install_log(&format!(
            "INSTALL REPAIR OK: {} — host port {} is listening after restart",
            package_id, port
        ))
        .await;
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Container {} is running but host port {} is not listening",
        container_name,
        port
    ))
}

async fn published_host_port(container_name: &str) -> Option<u16> {
    let output = tokio::process::Command::new("podman")
        .args(["port", container_name])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| {
        line.rsplit(':')
            .next()
            .and_then(|p| p.trim().parse::<u16>().ok())
    })
}

fn required_host_port(package_id: &str) -> Option<u16> {
    match package_id {
        "grafana" => Some(3000),
        "homeassistant" | "home-assistant" => Some(8123),
        "searxng" => Some(8888),
        "uptime-kuma" => Some(3002),
        "gitea" => Some(3001),
        "nextcloud" => Some(8085),
        "nginx-proxy-manager" => Some(8081),
        "portainer" => Some(9000),
        _ => None,
    }
}

async fn wait_for_host_port(package_id: &str, port: u16, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let ready = match package_id {
            "uptime-kuma" => http_host_port_ready(port, "/").await,
            _ => tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok(),
        };
        if ready {
            return true;
        }

        if std::time::Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn http_host_port_ready(port: u16, path: &str) -> bool {
    let Ok(Ok(mut stream)) = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    else {
        return false;
    };

    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }

    let mut buf = [0u8; 128];
    let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf)).await
    else {
        return false;
    };
    if n == 0 {
        return false;
    }
    let head = String::from_utf8_lossy(&buf[..n]);
    head.starts_with("HTTP/1.1 2")
        || head.starts_with("HTTP/1.1 3")
        || head.starts_with("HTTP/1.0 2")
        || head.starts_with("HTTP/1.0 3")
}

/// Resolve the host gateway IP for --add-host flag.
/// Resolve the default gateway IP from the routing table for --add-host flag.
/// Explicit IP avoids issues with "host-gateway" in rootless Podman.
async fn resolve_host_gateway() -> String {
    // Try `ip route` to get the default gateway
    if let Ok(output) = tokio::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .await
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("default") {
                if let Some(gw) = line.split_whitespace().nth(2) {
                    if !gw.is_empty() {
                        return format!("--add-host=host.containers.internal:{}", gw);
                    }
                }
            }
        }
    }
    // Fallback: try hostname -I (first IP)
    if let Ok(output) = tokio::process::Command::new("hostname")
        .args(["-I"])
        .output()
        .await
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(ip) = stdout.split_whitespace().next() {
            return format!("--add-host=host.containers.internal:{}", ip);
        }
    }
    // Last resort
    "--add-host=host.containers.internal:10.0.2.2".to_string()
}

async fn wait_for_adopted_container(package_id: &str, container_name: &str) -> Result<()> {
    for _ in 0..12u32 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let status = tokio::process::Command::new("podman")
            .args(["inspect", container_name, "--format", "{{.State.Status}}"])
            .output()
            .await;
        let Ok(output) = status else {
            continue;
        };
        let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match state.as_str() {
            "running" => return Ok(()),
            "exited" | "dead" => {
                let logs = tokio::process::Command::new("podman")
                    .args(["logs", "--tail", "40", container_name])
                    .output()
                    .await;
                let log_output = logs
                    .map(|o| {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        format!("{}{}", stdout, stderr)
                    })
                    .unwrap_or_default();
                install_log(&format!(
                    "INSTALL ADOPT CRASH: {} - existing container {} exited. Logs:\n{}",
                    package_id,
                    container_name,
                    &log_output.chars().take(1000).collect::<String>()
                ))
                .await;
                return Err(anyhow::anyhow!(
                    "Existing container {} exited after start. Logs: {}",
                    container_name,
                    log_output.chars().take(500).collect::<String>()
                ));
            }
            _ => {}
        }
    }

    install_log(&format!(
        "INSTALL ADOPT TIMEOUT: {} - existing container {} did not stay running",
        package_id, container_name
    ))
    .await;
    Err(anyhow::anyhow!(
        "Existing container {} did not reach running state within 60s",
        container_name
    ))
}

// bitcoin_data_volume_gb removed with write_bitcoin_conf: it only fed that
// function's volume-aware `prune=` line, which bitcoind never read either
// (see remove_stale_bitcoin_conf). The manifest's shell entrypoint already
// computes DISK_GB_VALUE and hardcodes -prune=50000 on small volumes — a
// real volume-aware prune fix belongs there, not in a conf file nothing
// reads. Tracked as follow-up in bitcoin-conf-crash-patch.md.

/// One-shot probe: does bitcoind answer an authenticated getblockchaininfo?
/// Works during IBD (the call answers with progress while syncing). Goes via
/// the host-published RPC port, which fails in exactly the same conditions
/// as the container-network path (bitcoind down, still binding, bad rpcbind).
async fn bitcoin_rpc_answering() -> bool {
    let (user, pass) = crate::bitcoin_rpc::bitcoin_rpc_credentials().await;
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let body = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "install-gate",
        "method": "getblockchaininfo",
        "params": [],
    });
    match client
        .post(crate::constants::BITCOIN_RPC_URL)
        .basic_auth(&user, Some(&pass))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Hold the install of a bitcoin-dependent app until bitcoind's RPC answers,
/// up to 3 minutes. No-op for apps that don't need bitcoin at start.
async fn wait_for_bitcoin_rpc_gate(package_id: &str) -> Result<()> {
    if !matches!(
        package_id,
        "lnd" | "electrumx" | "electrs" | "mempool-electrs" | "btcpay-server" | "btcpayserver"
    ) {
        return Ok(());
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let mut announced = false;
    while !bitcoin_rpc_answering().await {
        if tokio::time::Instant::now() >= deadline {
            install_log(&format!(
                "INSTALL FAIL: {} — Bitcoin RPC not answering after 180s; refusing to start a container that would crash-loop",
                package_id
            ))
            .await;
            anyhow::bail!(
                "Bitcoin's RPC is not responding, and {} needs it to start. \
                 Bitcoin may still be starting up — wait a minute and try again. \
                 If this persists, check the Bitcoin app logs.",
                package_id
            );
        }
        if !announced {
            install_log(&format!(
                "INSTALL WAIT: {} — waiting for Bitcoin RPC to become ready (up to 3 min)",
                package_id
            ))
            .await;
            announced = true;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    if announced {
        install_log(&format!(
            "INSTALL WAIT OK: {} — Bitcoin RPC is answering, starting container",
            package_id
        ))
        .await;
    }
    Ok(())
}

/// bitcoind reads only `/tmp/rpc.conf` + CLI args at container start (see
/// apps/bitcoin-core & bitcoin-knots manifest.yml, commit a597c1d9) — it
/// never reads a datadir bitcoin.conf. A leftover file from an older install
/// (or a manual edit) makes Bitcoin Core's own datadir-conflict safety check
/// refuse to start ("-conf=... vs default bitcoin.conf"). Remove it — via
/// the same host-privileged path the old writer/repairer used, since the
/// dir may already be chowned into the container's UID namespace by a
/// previous start — instead of "repairing" it into existence.
async fn remove_stale_bitcoin_conf() -> Result<bool> {
    let script = r#"
set -eu
conf=/var/lib/archipelago/bitcoin/bitcoin.conf
[ -f "$conf" ] || exit 0
mv "$conf" "$conf.disabled-$(date +%s)"
exit 2
"#;
    let status = host_sudo(&["sh", "-lc", script])
        .await
        .context("remove stale bitcoin.conf")?;
    match status.code() {
        Some(0) => Ok(false),
        Some(2) => {
            install_log(
                "INSTALL REPAIR: removed stale bitcoin.conf (conflicts with -conf=/tmp/rpc.conf launch)",
            )
            .await;
            Ok(true)
        }
        _ => Err(anyhow::anyhow!(
            "bitcoin.conf removal helper exited with {}",
            status
        )),
    }
}

/// Persist an install-time version selection for a multi-version app. Selecting
/// the catalog default (or a version equal to it) un-pins so the app tracks
/// latest; selecting any other version pins it. Best-effort: a write failure
/// just means the app installs at the catalog default.
async fn persist_install_version_selection(app_id: &str, version: &str) {
    use crate::container::version_config::{read, write, AppVersionConfig};
    let is_default = crate::container::app_catalog::catalog_default_version(app_id)
        .map(|d| d == version)
        .unwrap_or(false);
    let existing = read(app_id);
    let cfg = AppVersionConfig {
        pinned_version: if is_default {
            None
        } else {
            Some(version.to_string())
        },
        auto_update: existing.auto_update,
    };
    if let Err(e) = write(app_id, &cfg) {
        tracing::warn!(app_id, version, error = %e, "failed to persist install-time version selection");
    } else {
        tracing::info!(
            app_id,
            version,
            pinned = !is_default,
            "persisted install-time version selection"
        );
    }
}

/// Total bytes under `path` (bounded recursive walk). Used as a coarse
/// "bytes are moving" signal for pull stall detection — exact size doesn't
/// matter, only whether it CHANGES between polls. Errors count as 0.
fn dir_size_bytes(path: &str) -> u64 {
    fn walk(dir: &std::path::Path, depth: u32) -> u64 {
        if depth > 8 {
            return 0;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut total = 0u64;
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                total = total.saturating_add(walk(&entry.path(), depth + 1));
            } else {
                total = total.saturating_add(meta.len());
            }
        }
        total
    }
    walk(std::path::Path::new(path), 0)
}

fn should_try_orchestrator_install(package_id: &str, orchestrator_available: bool) -> bool {
    orchestrator_available && uses_orchestrator_install_flow(package_id)
}

fn orchestrator_install_app_id(package_id: &str) -> &str {
    match package_id {
        "electrs" | "mempool-electrs" => "electrumx",
        "home-assistant" => "homeassistant",
        _ => package_id,
    }
}

fn uses_orchestrator_install_flow(package_id: &str) -> bool {
    matches!(
        package_id,
        // Step 7 UI apps
        "bitcoin-ui"
            | "electrs-ui"
            | "lnd-ui"
            // Step 8b backend ports
            | "bitcoin-core"
            | "bitcoin-knots"
            | "lnd"
            | "fedimint"
            | "fedimint-gateway"
            | "filebrowser"
            | "electrumx"
            | "electrs"
            | "mempool-electrs"
            | "archy-mempool-db"
            | "mempool-api"
            | "archy-mempool-web"
            | "archy-btcpay-db"
            | "archy-nbxplorer"
            | "btcpay-server"
            | "homeassistant"
            | "home-assistant"
            | "nextcloud"
            | "vaultwarden"
            | "jellyfin"
            | "photoprism"
            | "uptime-kuma"
            | "gitea"
            | "portainer"
            | "meshtastic"
            // Build-backed user-facing app. Route it through the production
            // orchestrator so a fresh node builds its bundled image instead
            // of treating localhost/archipelago-source:local as a registry
            // image in the legacy installer.
            | "archipelago-source"
    )
}

fn is_unknown_app_id_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.to_string().contains("unknown app_id"))
}

#[cfg(test)]
mod tests {
    use super::{
        orchestrator_install_app_id, parse_setup_token, patched_indeedhub_nginx_config,
        should_try_orchestrator_install, uses_orchestrator_install_flow,
    };
    use crate::api::rpc::package::runtime::orchestrator_uninstall_app_ids;

    #[test]
    fn indeedhub_nginx_patch_is_complete_and_idempotent() {
        let original = r#"server {
    add_header X-Frame-Options SAMEORIGIN;
    location = /sw.js {
        expires off;
    }
    location /api/ {
        proxy_set_header X-Forwarded-Prefix /api;
    }
    location / {
        try_files $uri $uri/ /index.html;
        sub_filter_once on;
        sub_filter '</head>' '<script src="/nostr-provider.js"></script></head>';
    }
}
"#;
        let patched = patched_indeedhub_nginx_config(original);
        assert!(!patched.contains("X-Frame-Options"));
        assert!(patched.contains("location = /nostr-provider.js {"));
        assert!(patched.contains("Cache-Control \"no-cache, no-store, must-revalidate\""));
        assert!(patched.contains("src=\"/nostr-provider.js?v=tab-signer-v4\""));
        assert!(patched.contains("X-Forwarded-Prefix $http_x_forwarded_prefix/api"));
        assert_eq!(patched_indeedhub_nginx_config(&patched), patched);

        let previous_broker = patched.replace("tab-signer-v4", "tab-signer-v3");
        let migrated = patched_indeedhub_nginx_config(&previous_broker);
        assert!(migrated.contains("tab-signer-v4"));
        assert!(!migrated.contains("tab-signer-v3"));
        assert_eq!(patched_indeedhub_nginx_config(&migrated), migrated);
    }

    #[test]
    fn orchestrator_install_allowlist_includes_ported_backends() {
        for app in [
            "bitcoin-ui",
            "electrs-ui",
            "lnd-ui",
            "bitcoin-core",
            "bitcoin-knots",
            "lnd",
            "fedimint",
            "fedimint-gateway",
            "filebrowser",
            "electrumx",
            "electrs",
            "mempool-electrs",
            "archy-mempool-db",
            "mempool-api",
            "archy-mempool-web",
            "archy-btcpay-db",
            "archy-nbxplorer",
            "btcpay-server",
            "homeassistant",
            "home-assistant",
            "nextcloud",
            "vaultwarden",
            "jellyfin",
            "photoprism",
            "uptime-kuma",
            "gitea",
            "portainer",
            "meshtastic",
            "archipelago-source",
        ] {
            assert!(uses_orchestrator_install_flow(app));
            assert!(should_try_orchestrator_install(app, true));
        }
    }

    #[test]
    fn non_allowlisted_apps_stay_legacy_install() {
        for app in ["searxng", "mempool", "indeedhub", "immich", "penpot"] {
            assert!(!uses_orchestrator_install_flow(app));
            assert!(!should_try_orchestrator_install(app, true));
        }
    }

    #[test]
    fn missing_orchestrator_disables_orchestrator_install() {
        assert!(!should_try_orchestrator_install("bitcoin-ui", false));
    }

    #[test]
    fn install_aliases_map_to_manifest_app_ids() {
        assert_eq!(
            orchestrator_install_app_id("bitcoin-knots"),
            "bitcoin-knots"
        );
        assert_eq!(orchestrator_install_app_id("bitcoin-core"), "bitcoin-core");
        assert_eq!(orchestrator_install_app_id("electrs"), "electrumx");
        assert_eq!(orchestrator_install_app_id("mempool-electrs"), "electrumx");
        assert_eq!(
            orchestrator_install_app_id("home-assistant"),
            "homeassistant"
        );
        assert_eq!(orchestrator_install_app_id("lnd"), "lnd");
    }

    #[test]
    fn uninstall_aliases_map_to_exact_manifest_app_ids() {
        assert_eq!(
            orchestrator_uninstall_app_ids("bitcoin-knots"),
            vec!["bitcoin-knots", "bitcoin-ui"]
        );
        assert_eq!(
            orchestrator_uninstall_app_ids("electrs"),
            vec!["electrumx", "electrs-ui"]
        );
        assert_eq!(
            orchestrator_uninstall_app_ids("btcpay-server"),
            vec!["btcpay-server", "archy-nbxplorer", "archy-btcpay-db"]
        );
    }

    #[tokio::test]
    async fn companion_reconcile_aliases_include_ui_app_ids() {
        use crate::api::rpc::package::runtime::reconcile_companions_for;

        // Smoke only: unknown/non-companion apps are a no-op. Full companion
        // behavior is covered in container::companion tests; this guards that
        // the helper remains callable from install/start/restart paths.
        reconcile_companions_for("filebrowser").await;
    }

    #[test]
    fn missing_companion_is_ok_only_for_known_ui_companions() {
        use crate::api::rpc::package::runtime::is_missing_companion_ok;

        assert!(is_missing_companion_ok(
            "archy-bitcoin-ui",
            "Error: no container with name or ID \"archy-bitcoin-ui\" found"
        ));
        assert!(!is_missing_companion_ok(
            "bitcoin-knots",
            "Error: no container with name or ID \"bitcoin-knots\" found"
        ));
    }

    #[test]
    fn portainer_setup_token_is_extracted_from_log_lines() {
        // Shape captured live from portainer:2.45.0 on 2026-09-01 — the
        // token line is plain text inside the bordered s6 log block.
        let logs = [
            "2026/09/01 12:38PM INF github.com/portainer/portainer/api/database/boltdb/db.go:163 > loading PortainerDB | filename=portainer.db",
            "==========================",
            "setup_token=27637c02b6323972dff76bcad4caa456f957b521d3cfe3bc7fb95d2488dfd23a",
            "Paste it into the setup screen, or send it in the X-Setup-Token header.",
            "==========================",
        ];
        assert_eq!(
            parse_setup_token(&logs).as_deref(),
            Some("27637c02b6323972dff76bcad4caa456f957b521d3cfe3bc7fb95d2488dfd23a")
        );
    }

    #[test]
    fn portainer_setup_token_absent_when_setup_already_done() {
        // An instance with an existing admin account never prints the line —
        // the credentials card must not render a stale or empty token.
        let logs = [
            "2026/09/01 11:37AM INF api/datastore/migrator/migrate_ce.go:76 > db migrated to 2.45.0 |",
            "2026/09/01 11:37:38 server: Listening on http://0.0.0.0:8000",
        ];
        assert_eq!(parse_setup_token(&logs), None);
    }

    #[test]
    fn portainer_setup_token_rejects_short_or_non_hex_values() {
        assert_eq!(parse_setup_token(&["setup_token=abc123"]), None);
        assert_eq!(
            parse_setup_token(&["setup_token=".to_string().as_str()]),
            None
        );
    }
}
