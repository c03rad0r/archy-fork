// Docker Package Scanner
// Scans docker-compose containers and converts them to package data

use anyhow::Result;
use archipelago_container::{
    ContainerRuntime as ContainerRuntimeTrait, ContainerState, ContainerStatus, PodmanClient,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

use super::image_versions;
use crate::data_model::{
    Description, InstalledPackageDataEntry, InterfaceAddress, Interfaces, MainInterface, Manifest,
    PackageDataEntry, PackageState, ServiceStatus, StaticFiles,
};

/// One displayed package for each known container/manifest alias. Keep the
/// stopped-app restoration path in agreement with live-container discovery.
fn canonical_package_id(name: &str) -> &str {
    match name.strip_prefix("archy-").unwrap_or(name) {
        "immich_server" | "immich-server" => "immich",
        "immich-postgres" => "immich_postgres",
        "immich-redis" => "immich_redis",
        "mempool-web" | "mempool-frontend" => "mempool",
        name => name,
    }
}

pub struct DockerPackageScanner {
    runtime: Arc<dyn ContainerRuntimeTrait>,
}

impl DockerPackageScanner {
    pub fn new(runtime: Arc<dyn ContainerRuntimeTrait>) -> Self {
        Self { runtime }
    }

    /// Scan Docker containers and convert to package data
    pub async fn scan_containers(
        &self,
        data_dir: &std::path::Path,
        cached: &HashMap<String, PackageDataEntry>,
    ) -> Result<HashMap<String, PackageDataEntry>> {
        let mut containers = self.runtime.list_containers().await?;
        let installed = crate::crash_recovery::load_installed_apps(data_dir).await;
        let uninstalled = crate::crash_recovery::load_user_uninstalled(data_dir).await;
        restore_absent_installed(&mut containers, &installed, &uninstalled);

        debug!("Found {} containers", containers.len());

        let mut packages = HashMap::new();

        // Backend services that should not appear as apps
        let excluded_services = [
            "btcpay-db",
            "nbxplorer",
            "mempool-db",
            "mempool-api",
            "immich_postgres",
            "immich_redis",
            "endurain-db",
            "nextcloud-db",
            "indeedhub-api",
            "indeedhub-ffmpeg",
            "indeedhub-postgres",
            "indeedhub-redis",
            "indeedhub-minio",
            "indeedhub-relay",
            "indeedhub-build_api_1",
            "indeedhub-build_postgres_1",
            "indeedhub-build_redis_1",
            "indeedhub-build_minio_1",
            "indeedhub-build_minio-init_1",
            "indeedhub-build_relay_1",
            "indeedhub-build_ffmpeg-worker_1",
            "netbird-server",
            "netbird-dashboard",
            "pine-whisper",
            "pine-piper",
            "pine-openwakeword",
            "buildx_buildkit_default",
        ];

        // First pass: collect running UI containers. Custom UI-backed apps must
        // not advertise a launch URL unless their companion is actually alive.
        let mut ui_containers: HashMap<String, String> = HashMap::new();
        for container in &containers {
            if container.name.ends_with("-ui") {
                if !matches!(container.state, ContainerState::Running) {
                    continue;
                }
                // Map fedimint-ui -> fedimint, lnd-ui -> lnd (normalize archy- prefix for lookup)
                let parent_app = container
                    .name
                    .strip_suffix("-ui")
                    .unwrap_or(&container.name);
                let canonical_id = parent_app
                    .strip_prefix("archy-")
                    .unwrap_or(parent_app)
                    .to_string();
                let ui_address = extract_lan_address(&container.ports)
                    .or_else(|| companion_lan_address(&canonical_id));
                if let Some(ui_address) = ui_address {
                    ui_containers.insert(canonical_id, ui_address);
                }
            }
        }

        debug!("Found {} UI containers", ui_containers.len());

        for container in containers {
            // Use the same alias mapping as stopped-app restoration.
            let app_id = canonical_package_id(&container.name).to_owned();

            // Skip backend services (databases, APIs, etc.)
            if excluded_services.contains(&app_id.as_str()) {
                debug!("Skipping backend service: {}", app_id);
                continue;
            }

            if is_transient_podman_helper(&app_id, &container.ports) {
                debug!("Skipping transient Podman helper container: {}", app_id);
                continue;
            }

            // Skip podman-compose infrastructure containers (e.g. indeedhub-build_api_1)
            // These have the project prefix pattern: {project}_{service}_{instance}
            if app_id.starts_with("indeedhub-build_") {
                debug!("Skipping IndeedHub compose service: {}", app_id);
                continue;
            }

            if app_id.starts_with("buildx_buildkit") {
                debug!("Skipping BuildKit helper container: {}", app_id);
                continue;
            }

            // Skip UI containers (they're merged with their parent apps)
            if app_id.ends_with("-ui") {
                debug!("Skipping UI container: {}", app_id);
                continue;
            }

            if container.id.is_empty() {
                if let Some(previous) = cached.get(&app_id) {
                    let mut held = previous.clone();
                    held.state = PackageState::Stopped;
                    held.ui_ready = Some(false);
                    held.health = None;
                    held.exit_code = None;
                    packages.insert(app_id.clone(), held);
                    continue;
                }
            }

            // Get metadata for this app
            let metadata = get_app_metadata(&app_id);
            // Manifest-owned metadata (icon) wins over the static table: the
            // manifest is what the catalog signed and what the App Store shows,
            // so it is also what an installed tile must render.
            let manifest_icon = real_manifest_metadata(&app_id)
                .and_then(|m| m.get("icon").and_then(|v| v.as_str()).map(str::to_string))
                .filter(|s| !s.trim().is_empty());

            // Resolve UI address: separate UI containers > static map > dynamic ports
            let lan_address = if app_id == "netbird" {
                reachable_lan_address(&app_id, netbird_configured_launch_url().await).await
            } else if let Some(ui_address) = ui_containers.get(&app_id) {
                // Apps with separate UI containers (e.g. archy-bitcoin-ui, archy-lnd-ui)
                debug!("Using UI container for {}: {}", app_id, ui_address);
                reachable_lan_address(&app_id, Some(ui_address.clone())).await
            } else {
                // Prefer the known web UI port over arbitrary first binding
                // (for example Gitea exposes SSH on 2222 before web on 3001).
                let candidate = package_launch_candidate(
                    &app_id,
                    &container.ports,
                    PodmanClient::lan_address_for(&app_id),
                );
                reachable_lan_address(&app_id, candidate).await
            };

            debug!(
                "Container {}: ports={:?}, lan_address={:?}",
                app_id, container.ports, lan_address
            );

            // Convert container state to package/service state
            let (package_state, service_status) = convert_state(&container.state);

            let tor_address = read_tor_address(&app_id).await;

            // Extract actual version from container image tag
            let running_version = if container.id.is_empty() {
                String::new() // Absence cannot establish the installed image version.
            } else {
                image_versions::extract_version_from_image(&container.image)
            };

            // Decoupled from the binary OTA: prefer the remote app catalog,
            // falling back to the image-versions.sh pin when uncovered/offline.
            let available_update = if container.id.is_empty() {
                None
            } else {
                crate::container::app_catalog::available_update_for_app(&app_id, &container.image)
            };

            let package = PackageDataEntry {
                ui_ready: Some(false),
                state: package_state.clone(),
                health: container.health.clone(),
                exit_code: if package_state == PackageState::Exited {
                    container.exit_code
                } else {
                    None
                },
                static_files: StaticFiles {
                    license: "MIT".to_string(),
                    instructions: metadata.description.clone(),
                    icon: manifest_icon.unwrap_or_else(|| metadata.icon.clone()),
                },
                manifest: Manifest {
                    id: app_id.clone(),
                    title: metadata.title.clone(),
                    version: running_version,
                    description: Description {
                        short: metadata.description.clone(),
                        long: metadata.description.clone(),
                    },
                    release_notes: "Docker container".to_string(),
                    license: "MIT".to_string(),
                    wrapper_repo: metadata.repo.clone(),
                    upstream_repo: metadata.repo.clone(),
                    support_site: metadata.repo.clone(),
                    marketing_site: metadata.repo.clone(),
                    donation_url: None,
                    author: Some("Archipelago".to_string()),
                    website: lan_address.clone(),
                    tier: Some(metadata.tier.to_string()),
                    interfaces: {
                        // `ui` is no longer implied by a published port: a
                        // headless backend with an exposed port is a service,
                        // not a launchable app. ui_detection consults the
                        // manifest declaration first, then HTTP-probes the
                        // port. A DECLARED UI classifies the app as launchable
                        // even when no reachable address was confirmed this
                        // scan — the launch button falls back to the static
                        // port map, and burying a manifest-declared UI app
                        // (Alby Hub) in Services because a probe missed was
                        // exactly the classification bug this fixes.
                        let has_ui = super::ui_detection::has_web_ui(
                            &app_id,
                            lan_address.as_deref(),
                            package_state == PackageState::Running,
                        )
                        .await;
                        if lan_address.is_some() || tor_address.is_some() || has_ui {
                            Some(Interfaces {
                                main: Some(MainInterface {
                                    ui: has_ui.then(|| "true".to_string()),
                                    tor_config: tor_address.clone(),
                                    lan_config: None,
                                }),
                            })
                        } else {
                            None
                        }
                    },
                },
                available_update,
                installed: Some(InstalledPackageDataEntry {
                    current_dependents: HashMap::new(),
                    current_dependencies: HashMap::new(),
                    last_backup: None,
                    interface_addresses: if lan_address.is_some() || tor_address.is_some() {
                        let mut addresses = HashMap::new();
                        // Only include tor_address if we have a real v3 .onion (not placeholder)
                        let tor = tor_address
                            .filter(|s| is_real_onion_address(s))
                            .unwrap_or_default();
                        addresses.insert(
                            "main".to_string(),
                            InterfaceAddress {
                                tor_address: tor,
                                lan_address,
                            },
                        );
                        addresses
                    } else {
                        HashMap::new()
                    },
                    status: service_status,
                }),
                install_progress: None,
                uninstall_stage: None,
            };

            packages.insert(app_id.clone(), package);
            info!(
                "Detected container: {} ({})",
                metadata.title,
                package_state_str(&package_state)
            );
        }

        let probes: Vec<_> = packages
            .iter()
            .filter_map(|(id, pkg)| {
                if pkg.state != PackageState::Running {
                    return None;
                }
                let url = pkg
                    .installed
                    .as_ref()?
                    .interface_addresses
                    .get("main")?
                    .lan_address
                    .clone()?;
                Some((id.clone(), url))
            })
            .collect();
        let mut results = futures_util::stream::iter(
            probes
                .into_iter()
                .map(|(id, url)| async move { (id, launch_http_ready(&url).await) }),
        )
        .buffer_unordered(8);
        while let Some((id, ready)) = results.next().await {
            if let Some(pkg) = packages.get_mut(&id) {
                pkg.ui_ready = Some(ready);
            }
        }
        // HTTP on loopback can precede the LAN/Tor listener after install.
        let port_map = crate::appgate::identity::build_port_map();
        let gated: Vec<_> = packages
            .iter()
            .filter_map(|(id, pkg)| {
                if pkg.ui_ready != Some(true) {
                    return None;
                }
                let url = pkg
                    .installed
                    .as_ref()?
                    .interface_addresses
                    .get("main")?
                    .lan_address
                    .as_deref()?;
                let port = launch_url_port(url)?;
                port_map
                    .gated(port)
                    .filter(|gate| gate.declared)
                    .map(|_| (id.clone(), port))
            })
            .collect();
        if !gated.is_empty() {
            use crate::appgate::listener::{port_claimed, refresh_now, shared_status};
            let status = shared_status();
            let needs_refresh = {
                let current = status.read().await;
                gated.iter().any(|(_, port)| !port_claimed(&current, *port))
            };
            if needs_refresh {
                refresh_now().await;
            }
            let current = status.read().await;
            for (id, port) in gated {
                if !port_claimed(&current, port) {
                    packages.get_mut(&id).unwrap().ui_ready = Some(false);
                }
            }
        }
        Ok(packages)
    }
}

/// Quadlet removes containers during ordinary stops/restarts. Rebuild installed
/// entries even on the daemon's first scan; a runtime absence is not uninstall.
fn restore_absent_installed(
    containers: &mut Vec<ContainerStatus>,
    installed: &std::collections::HashSet<String>,
    uninstalled: &std::collections::HashSet<String>,
) {
    let mut present: std::collections::HashSet<String> = containers
        .iter()
        .map(|c| canonical_package_id(&c.name).to_owned())
        .collect();
    let removed: std::collections::HashSet<_> = uninstalled
        .iter()
        .map(|id| canonical_package_id(id))
        .collect();
    for name in installed {
        let id = canonical_package_id(name);
        if removed.contains(id) || !present.insert(id.to_owned()) {
            continue;
        }
        containers.push(ContainerStatus {
            id: String::new(),
            name: id.to_owned(),
            state: ContainerState::Stopped,
            health: None,
            exit_code: None,
            started_at: None,
            image: String::new(),
            created: String::new(),
            ports: Vec::new(),
            lan_address: None,
        });
    }
}

/// Probe the actual loopback upstream, not the app gate's login page. A bound
/// TCP socket alone can still reset requests or serve a startup 503.
async fn launch_http_ready(candidate: &str) -> bool {
    let Ok(mut url) = reqwest::Url::parse(candidate) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    if url.set_host(Some("127.0.0.1")).is_err() {
        return false;
    }
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            // Self-signed local app certificates are normal. This client only
            // contacts loopback and never sends credentials or follows redirects.
            .danger_accept_invalid_certs(true)
            .build()
            .expect("local readiness client")
    });
    match client.get(url).send().await {
        Ok(response) => matches!(response.status().as_u16(), 200..=399 | 401 | 403),
        Err(_) => false,
    }
}

#[cfg(test)]
mod lifecycle_regression_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn immich_dependency_aliases_share_the_hidden_component_ids() {
        for id in [
            "immich-postgres",
            "immich_postgres",
            "archy-immich-postgres",
        ] {
            assert_eq!(canonical_package_id(id), "immich_postgres");
        }
        assert_eq!(canonical_package_id("immich-redis"), "immich_redis");
        assert_eq!(canonical_package_id("immich-server"), "immich");
    }

    #[test]
    fn registry_survives_empty_runtime_and_deduplicates_aliases() {
        let installed = ["archy-gitea", "gitea", "immich_server", "archy-removed"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let removed = ["removed".to_owned()].into_iter().collect();
        let mut containers = Vec::new();
        restore_absent_installed(&mut containers, &installed, &removed);
        assert_eq!(containers.len(), 2);
        assert!(containers
            .iter()
            .all(|c| c.state == ContainerState::Stopped));
        containers[0].state = ContainerState::Running;
        restore_absent_installed(&mut containers, &installed, &removed);
        assert_eq!(containers.len(), 2);
        assert_eq!(containers[0].state, ContainerState::Running);
    }

    #[test]
    fn mempool_frontend_inventory_alias_does_not_create_a_second_package() {
        let installed = ["mempool", "archy-mempool-web", "mempool-web"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut containers = Vec::new();
        restore_absent_installed(&mut containers, &installed, &Default::default());
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].name, "mempool");
        containers[0].id = "live-frontend".into();
        containers[0].state = ContainerState::Running;
        restore_absent_installed(&mut containers, &installed, &Default::default());
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].id, "live-frontend");
        assert_eq!(containers[0].state, ContainerState::Running);
        assert_eq!(canonical_package_id("archy-mempool-web"), "mempool");
        assert_eq!(canonical_package_id("mempool-api"), "mempool-api");
        containers.clear();
        restore_absent_installed(
            &mut containers,
            &installed,
            &["mempool".into()].into_iter().collect(),
        );
        assert!(containers.is_empty());
    }

    #[tokio::test]
    async fn readiness_rejects_startup_errors_and_accepts_auth_and_redirects() {
        for (status, expected) in [
            (200, true),
            (302, true),
            (401, true),
            (403, true),
            (404, false),
            (500, false),
            (502, false),
            (503, false),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let task = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0; 2048];
                let n = stream.read(&mut buf).await.unwrap();
                assert!(String::from_utf8_lossy(&buf[..n]).starts_with("GET /start HTTP/1.1"));
                stream.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            });
            assert_eq!(
                launch_http_ready(&format!("http://localhost:{port}/start")).await,
                expected,
                "status {status}"
            );
            task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn readiness_rejects_tcp_accept_without_http() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
        });
        assert!(!launch_http_ready(&format!("http://localhost:{port}/")).await);
        task.await.unwrap();
        assert!(!launch_http_ready(&format!("http://localhost:{port}/")).await);
        assert!(!launch_http_ready("file:///tmp/test").await);
    }
}

struct AppMetadata {
    title: String,
    description: String,
    icon: String,
    repo: String,
    tier: &'static str,
}

/// Get the app tier: "core", "recommended", or "optional".
fn get_app_tier(app_id: &str) -> &'static str {
    match app_id {
        // Core: required for basic Bitcoin node
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => "core",
        "lnd" => "core",
        "mempool" | "mempool-web" | "mempool-api" | "electrumx" | "mempool-electrs" | "electrs" => {
            "core"
        }
        "btcpay" | "btcpay-server" | "btcpayserver" => "core",
        "filebrowser" => "core",
        // Recommended: enhanced functionality
        "fedimint" | "fedimint-gateway" => "recommended",
        "vaultwarden" => "recommended",
        "uptime-kuma" => "recommended",
        "grafana" => "recommended",
        "searxng" => "recommended",
        "tailscale" | "netbird" => "recommended",
        "portainer" => "recommended",
        // Optional: everything else
        _ => "optional",
    }
}

fn is_transient_podman_helper(app_id: &str, ports: &[String]) -> bool {
    if !ports.is_empty() {
        return false;
    }

    let Some((left, right)) = app_id.split_once('_') else {
        return false;
    };

    !left.is_empty()
        && !right.is_empty()
        && left.chars().all(|c| c.is_ascii_lowercase())
        && right.chars().all(|c| c.is_ascii_lowercase())
}

/// Raw `metadata` block of an installed app's real manifest — catalog overlay
/// first (origin-wins), disk manifest as fallback. Kept as raw JSON because
/// the typed `AppManifest` deliberately does not model `metadata`, yet its
/// `icon` is what makes an installed app's tile render the right icon on
/// every surface (My Apps, Services, launcher, companion) instead of the
/// generic A-mark — the exact regression Cuprate exposed on install.
fn real_manifest_metadata(app_id: &str) -> Option<serde_json::Value> {
    for (id, value) in crate::container::app_catalog::catalog_manifest_values() {
        if id == app_id {
            return value.get("app").and_then(|a| a.get("metadata")).cloned();
        }
    }
    let mut candidates = Vec::new();
    if let Ok(dir) = std::env::var("ARCHIPELAGO_DATA_DIR") {
        candidates.push(
            std::path::PathBuf::from(dir)
                .join("../apps")
                .join(app_id)
                .join("manifest.yml"),
        );
    }
    candidates.push(
        std::path::PathBuf::from("/opt/archipelago/apps")
            .join(app_id)
            .join("manifest.yml"),
    );
    for path in candidates {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_yaml::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let meta = value.get("app").and_then(|a| a.get("metadata")).cloned();
        if meta.is_some() {
            return meta;
        }
    }
    None
}

fn get_app_metadata(app_id: &str) -> AppMetadata {
    let mut meta = match app_id {
        "bitcoin-core" => AppMetadata {
            title: "Bitcoin Core".to_string(),
            description: "Reference Bitcoin node implementation".to_string(),
            icon: "/assets/img/app-icons/bitcoin-core.svg".to_string(),
            repo: "https://github.com/bitcoin/bitcoin".to_string(),
            tier: "",
        },
        "bitcoin" | "bitcoin-knots" => AppMetadata {
            title: "Bitcoin Knots".to_string(),
            description: "Enhanced Bitcoin node implementation".to_string(),
            icon: "/assets/img/app-icons/bitcoin-knots.webp".to_string(),
            repo: "https://github.com/bitcoinknots/bitcoin".to_string(),
            tier: "",
        },
        "btcpay" | "btcpay-server" | "btcpayserver" => AppMetadata {
            title: "BTCPay Server".to_string(),
            description: "Self-hosted Bitcoin payment processor".to_string(),
            icon: "/assets/img/app-icons/btcpay-server.png".to_string(),
            repo: "https://github.com/btcpayserver/btcpayserver".to_string(),
            tier: "",
        },
        "homeassistant" | "home-assistant" => AppMetadata {
            title: "Home Assistant".to_string(),
            description: "Open source home automation platform".to_string(),
            icon: "/assets/img/app-icons/homeassistant.png".to_string(),
            repo: "https://github.com/home-assistant/core".to_string(),
            tier: "",
        },
        "grafana" => AppMetadata {
            title: "Grafana".to_string(),
            description: "Analytics and monitoring platform".to_string(),
            icon: "/assets/img/app-icons/grafana.png".to_string(),
            repo: "https://github.com/grafana/grafana".to_string(),
            tier: "",
        },
        "endurain" => AppMetadata {
            title: "Endurain".to_string(),
            description: "Self-hosted fitness tracking platform".to_string(),
            icon: "/assets/img/app-icons/endurain.png".to_string(),
            repo: "https://github.com/joaovitoriasilva/endurain".to_string(),
            tier: "",
        },
        "fedimint" | "fedimintd" => AppMetadata {
            title: "Fedimint Guardian".to_string(),
            description: "Federated Bitcoin mint — Guardian node for federation consensus".to_string(),
            icon: "/assets/img/app-icons/fedimint.png".to_string(),
            repo: "https://github.com/fedimint/fedimint".to_string(),
            tier: "",
        },
        "fedimint-gateway" => AppMetadata {
            title: "Fedimint Gateway".to_string(),
            description: "Fedimint Lightning gateway for ecash payments".to_string(),
            icon: "/assets/img/app-icons/fedimint.png".to_string(),
            repo: "https://github.com/fedimint/fedimint".to_string(),
            tier: "",
        },
        "fedimint-clientd" | "fmcd" => AppMetadata {
            title: "Fedimint Client".to_string(),
            description: "Fedimint ecash client daemon (fmcd) — lets your node hold Fedimint ecash and join federations".to_string(),
            icon: "/assets/img/app-icons/fedimint.png".to_string(),
            repo: "https://github.com/minmoto/fmcd".to_string(),
            tier: "",
        },
        "barkd" | "bark" => AppMetadata {
            title: "Ark Wallet".to_string(),
            description: "Ark protocol wallet daemon (barkd) — self-custodial off-chain bitcoin via an Ark server (signet)".to_string(),
            icon: "/assets/img/app-icons/bark.png".to_string(),
            repo: "https://gitlab.com/ark-bitcoin/bark".to_string(),
            tier: "",
        },
        "morphos" | "morphos-server" => AppMetadata {
            title: "Morphos".to_string(),
            description: "Self-hosted file converter".to_string(),
            icon: "/assets/img/app-icons/morphos.png".to_string(),
            repo: "https://github.com/danvergara/morphos".to_string(),
            tier: "",
        },
        "lnd" | "lightning-stack" => AppMetadata {
            title: "LND".to_string(),
            description: "Lightning Network Daemon".to_string(),
            icon: "/assets/img/app-icons/lnd.png".to_string(),
            repo: "https://github.com/lightningnetwork/lnd".to_string(),
            tier: "",
        },
        "mempool" | "mempool-web" => AppMetadata {
            title: "Mempool".to_string(),
            description: "Bitcoin blockchain explorer".to_string(),
            icon: "/assets/img/app-icons/mempool.webp".to_string(),
            repo: "https://github.com/mempool/mempool".to_string(),
            tier: "",
        },
        "electrumx" | "mempool-electrs" | "electrs" => AppMetadata {
            title: "ElectrumX".to_string(),
            description: "ElectrumX server — full Electrum protocol indexer for Bitcoin. Powers Mempool and Electrum wallets.".to_string(),
            icon: "/assets/img/app-icons/electrumx.png".to_string(),
            repo: "https://github.com/spesmilo/electrumx".to_string(),
            tier: "",
        },
        "ollama" => AppMetadata {
            title: "Ollama".to_string(),
            description: "Run large language models locally".to_string(),
            icon: "/assets/img/app-icons/ollama.png".to_string(),
            repo: "https://github.com/ollama/ollama".to_string(),
            tier: "",
        },
        "monerod" | "monero" => AppMetadata {
            title: "Monero".to_string(),
            description: "Private cryptocurrency full node (Monero)".to_string(),
            icon: "/assets/img/app-icons/monero.png".to_string(),
            repo: "https://github.com/monero-project/monero".to_string(),
            tier: "",
        },
        "elementsd" | "liquid" => AppMetadata {
            title: "Liquid Network".to_string(),
            description: "Bitcoin sidechain for confidential transactions and faster settlements".to_string(),
            icon: "/assets/img/app-icons/liquid.png".to_string(),
            repo: "https://github.com/ElementsProject/elements".to_string(),
            tier: "",
        },
        "searxng" => AppMetadata {
            title: "SearXNG".to_string(),
            description: "Privacy-respecting metasearch engine".to_string(),
            icon: "/assets/img/app-icons/searxng.png".to_string(),
            repo: "https://github.com/searxng/searxng".to_string(),
            tier: "",
        },
        "cryptpad" => AppMetadata {
            title: "CryptPad".to_string(),
            description: "End-to-end encrypted document collaboration".to_string(),
            icon: "/assets/img/app-icons/cryptpad.webp".to_string(),
            repo: "https://github.com/cryptpad/cryptpad".to_string(),
            tier: "",
        },
        "nextcloud" => AppMetadata {
            title: "Nextcloud".to_string(),
            description: "Self-hosted cloud storage and file management".to_string(),
            icon: "/assets/img/app-icons/nextcloud.webp".to_string(),
            repo: "https://github.com/nextcloud/server".to_string(),
            tier: "",
        },
        "vaultwarden" => AppMetadata {
            title: "Vaultwarden".to_string(),
            description: "Self-hosted password manager (Bitwarden compatible)".to_string(),
            icon: "/assets/img/app-icons/vaultwarden.webp".to_string(),
            repo: "https://github.com/dani-garcia/vaultwarden".to_string(),
            tier: "",
        },
        "jellyfin" => AppMetadata {
            title: "Jellyfin".to_string(),
            description: "Free media server system".to_string(),
            icon: "/assets/img/app-icons/jellyfin.webp".to_string(),
            repo: "https://github.com/jellyfin/jellyfin".to_string(),
            tier: "",
        },
        "photoprism" => AppMetadata {
            title: "PhotoPrism".to_string(),
            description: "AI-powered photo management".to_string(),
            icon: "/assets/img/app-icons/photoprism.svg".to_string(),
            repo: "https://github.com/photoprism/photoprism".to_string(),
            tier: "",
        },
        "immich" | "immich_server" => AppMetadata {
            title: "Immich".to_string(),
            description: "High-performance self-hosted photo and video backup".to_string(),
            icon: "/assets/img/app-icons/immich.png".to_string(),
            repo: "https://github.com/immich-app/immich".to_string(),
            tier: "",
        },
        "filebrowser" => AppMetadata {
            title: "File Browser".to_string(),
            description: "Web-based file manager".to_string(),
            icon: "/assets/img/app-icons/file-browser.webp".to_string(),
            repo: "https://github.com/filebrowser/filebrowser".to_string(),
            tier: "",
        },
        "nginx-proxy-manager" => AppMetadata {
            title: "Nginx Proxy Manager".to_string(),
            description: "Easy proxy management with SSL".to_string(),
            icon: "/assets/img/app-icons/nginx.svg".to_string(),
            repo: "https://github.com/NginxProxyManager/nginx-proxy-manager".to_string(),
            tier: "",
        },
        "portainer" => AppMetadata {
            title: "Portainer".to_string(),
            description: "Container management UI".to_string(),
            icon: "/assets/img/app-icons/portainer.webp".to_string(),
            repo: "https://github.com/portainer/portainer".to_string(),
            tier: "",
        },
        "uptime-kuma" => AppMetadata {
            title: "Uptime Kuma".to_string(),
            description: "Self-hosted monitoring tool".to_string(),
            icon: "/assets/img/app-icons/uptime-kuma.webp".to_string(),
            repo: "https://github.com/louislam/uptime-kuma".to_string(),
            tier: "",
        },
        "tailscale" => AppMetadata {
            title: "Tailscale".to_string(),
            description: "Zero-config VPN for secure remote access".to_string(),
            icon: "/assets/img/app-icons/tailscale.webp".to_string(),
            repo: "https://github.com/tailscale/tailscale".to_string(),
            tier: "",
        },
        "netbird" => AppMetadata {
            title: "NetBird".to_string(),
            description: "Self-hosted WireGuard mesh VPN control plane and dashboard".to_string(),
            icon: "/assets/img/app-icons/netbird.svg".to_string(),
            repo: "https://github.com/netbirdio/netbird".to_string(),
            tier: "",
        },
        "gitea" => AppMetadata {
            title: "Gitea".to_string(),
            description: "Self-hosted Git service with repository and package hosting".to_string(),
            icon: "/assets/img/app-icons/gitea.svg".to_string(),
            repo: "https://gitea.com".to_string(),
            tier: "",
        },
        "indeedhub" | "indeehub" => AppMetadata {
            title: "IndeedHub".to_string(),
            description: "Decentralized media streaming platform".to_string(),
            icon: "/assets/img/app-icons/indeedhub.png".to_string(),
            repo: "https://github.com/indeedhub/indeedhub".to_string(),
            tier: "",
        },
        "tor" | "archy-tor" => AppMetadata {
            title: "Tor".to_string(),
            description: "Anonymous overlay network for privacy".to_string(),
            icon: "/assets/img/app-icons/tor.svg".to_string(),
            repo: "https://gitlab.torproject.org/tpo/core/tor".to_string(),
            tier: "",
        },
        "botfights" => AppMetadata {
            title: "BotFights".to_string(),
            description: "AI bot arena — build, train, and battle autonomous agents".to_string(),
            icon: "/assets/img/app-icons/botfights.svg".to_string(),
            repo: "https://botfights.net".to_string(),
            tier: "",
        },
        _ => AppMetadata {
            title: app_id.to_string(),
            description: format!("{} application", app_id),
            icon: "/assets/img/favico.png".to_string(),
            repo: "#".to_string(),
            tier: "",
        },
    };
    apply_dynamic_metadata(app_id, &mut meta);
    meta.tier = get_app_tier(app_id);
    meta
}

fn apply_dynamic_metadata(app_id: &str, meta: &mut AppMetadata) {
    let config_path = format!("/var/lib/archipelago/app-configs/{}.json", app_id);
    let Ok(data) = std::fs::read_to_string(config_path) else {
        return;
    };
    let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&data) else {
        return;
    };
    if let Some(title) = cfg
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 80)
    {
        meta.title = title.to_string();
    }
    if let Some(description) = cfg
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 240)
    {
        meta.description = description.to_string();
    }
}

/// Map app_id to Tor hidden service directory name.
/// "archipelago" is the main web UI (nginx port 80).
/// Supports container names from deploy (archy-*, btcpay-server, etc.).
///
/// This must match what enrollment actually names the hidden service dir
/// with — both the install-time auto-enroll (`install.rs`) and the manual
/// `tor.create-service` RPC write `HiddenServiceDir` using the raw
/// `package_id`/`name` verbatim, with no canonicalization. So `bitcoin-core`
/// gets its own identity arm rather than folding into the "bitcoin" alias:
/// aliasing it here without also canonicalizing the write side would point
/// this lookup at `hidden_service_bitcoin`, which never gets created — the
/// on-disk dir is always `hidden_service_bitcoin-core` for this app id.
fn tor_service_name(app_id: &str) -> Option<&'static str> {
    match app_id {
        "archipelago" => Some("archipelago"),
        "bitcoin-core" => Some("bitcoin-core"),
        "bitcoin" | "bitcoin-knots" | "bitcoind" => Some("bitcoin"),
        "electrumx" | "electrs" | "electrum" => Some("electrumx"),
        "lnd" | "lnd-ui" => Some("lnd"),
        "btcpay" | "btcpay-server" | "btcpayserver" => Some("btcpay"),
        "mempool" | "mempool-web" | "mempool-frontend" => Some("mempool"),
        "fedimint" | "fedimint-gateway" => Some("fedimint"),
        "filebrowser" => Some("filebrowser"),
        _ => None,
    }
}

/// V3 onion addresses are 56 base32 chars + ".onion". Placeholders like "btcpay.onion" are not real.
fn is_real_onion_address(s: &str) -> bool {
    s.ends_with(".onion") && s.len() >= 60 && s.len() <= 70
}

/// Read real .onion address from Tor hidden service hostname file.
/// Service name "archipelago" is for the main web UI (nginx port 80).
/// Uses TOR_DATA_DIR env var if set, else /var/lib/archipelago/tor.
pub async fn read_tor_address(app_id: &str) -> Option<String> {
    let service = tor_service_name(app_id)?;
    let base =
        std::env::var("TOR_DATA_DIR").unwrap_or_else(|_| "/var/lib/archipelago/tor".to_string());

    // Try readable hostname copy first (when system Tor owns hidden_service dirs)
    let hostnames_path = std::path::Path::new(&base)
        .parent()
        .unwrap_or(std::path::Path::new("/var/lib/archipelago"))
        .join("tor-hostnames")
        .join(service);
    if let Some(addr) = tokio::fs::read_to_string(&hostnames_path)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| s.ends_with(".onion") && !s.is_empty())
    {
        return Some(addr);
    }

    // Fall back to hidden_service directory
    let path = std::path::Path::new(&base)
        .join(format!("hidden_service_{}", service))
        .join("hostname");
    tokio::fs::read_to_string(&path)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| s.ends_with(".onion") && !s.is_empty())
}

/// Container-side ports that are essentially never a web UI, even when
/// published alongside one — e.g. gitea publishes SSH (`2222->22`) before its
/// web port (`3001->3000`), and podman's port list order isn't guaranteed to
/// put the UI port first. Skipping these lets launch-URL guessing work for
/// any future multi-port app without a per-app static override.
const NON_HTTP_CONTAINER_PORTS: &[&str] = &["22", "21", "3306", "5432", "6379", "27017"];

fn extract_lan_address(ports: &[String]) -> Option<String> {
    let mut first_candidate = None;
    for port_str in ports {
        // Parse port strings like "0.0.0.0:18443->18443/tcp" or "0.0.0.0:18443-18444->18443-18444/tcp"
        let Some((public_part, _)) = port_str.split_once("->") else {
            continue;
        };
        let Some((_, port_part)) = public_part.rsplit_once(':') else {
            continue;
        };
        // Extract just the first port if it's a range (e.g., "18443-18444" -> "18443")
        let host_port = port_part.split('-').next().unwrap_or(port_part);
        let Ok(host_port) = host_port.parse::<u16>() else {
            continue;
        };
        if host_port == 0 {
            continue;
        }
        let candidate = format!("http://localhost:{}", host_port);
        if first_candidate.is_none() {
            first_candidate = Some(candidate.clone());
        }

        let container_port = port_str
            .split("->")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .map(|s| s.split('-').next().unwrap_or(s));
        if container_port.is_some_and(|p| NON_HTTP_CONTAINER_PORTS.contains(&p)) {
            continue;
        }
        return Some(candidate);
    }
    // Nothing looked HTTP-like — fall back to whatever was published first
    // rather than reporting no launch URL at all.
    first_candidate
}

/// netbird's dashboard launch URL: HTTPS on 8087 (the proxy terminates TLS —
/// the dashboard needs a secure context for OIDC PKCE, issue #15) at the node's
/// primary host IP so it's reachable from the LAN. Manifest-driven netbird no
/// longer writes `dashboard.env`, so this is derived from host facts (the same
/// `{{HOST_IP}}` the orchestrator bakes into the cert/config); it falls back to
/// the static localhost mapping when the host IP can't be read. URL shape is
/// identical to the legacy installer's, so the existing https reachability
/// wrapper still applies.
async fn netbird_configured_launch_url() -> Option<String> {
    if let Some(ip) = first_host_ip().await {
        return Some(format!("https://{ip}:8087"));
    }
    PodmanClient::lan_address_for("netbird")
}

/// The node's primary host IP. Mirrors the orchestrator's `detect_host_ip`
/// so launch URLs match the cert/config the orchestrator renders for
/// `{{HOST_IP}}`.
async fn first_host_ip() -> Option<String> {
    crate::host_ip::primary_host_ipv4().await
}

async fn reachable_lan_address(app_id: &str, candidate: Option<String>) -> Option<String> {
    let url = candidate?;
    if !requires_reachable_launch(app_id) {
        return Some(url);
    }
    let Some(port) = launch_url_port(&url) else {
        return None;
    };
    if launch_port_reachable(port).await {
        Some(url)
    } else {
        debug!(app_id = %app_id, port, "suppressing unreachable launch URL");
        None
    }
}

/// Extract the TCP port from a launch URL's authority.
///
/// The candidate URL can carry a path when it comes from a manifest
/// `interfaces.main` declaration (e.g. `http://localhost:8096/`). A naive
/// `rsplit(':')` then yields `"8096/"`, which fails to parse and silently
/// drops a reachable launch URL. Reading digits after the final colon mirrors
/// `port_from_url` in the RPC layer and tolerates a trailing path.
pub(crate) fn launch_url_port(url: &str) -> Option<u16> {
    let after_colon = url.rsplit_once(':')?.1;
    after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<u16>()
        .ok()
}

async fn launch_port_reachable(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await,
        Ok(Ok(_))
    )
}

fn requires_reachable_launch(app_id: &str) -> bool {
    matches!(
        app_id,
        "botfights"
            | "btcpay-server"
            | "fedimint"
            | "filebrowser"
            | "grafana"
            | "homeassistant"
            | "home-assistant"
            | "jellyfin"
            | "mempool"
            | "nginx-proxy-manager"
            | "uptime-kuma"
            | "gitea"
            | "nextcloud"
            | "portainer"
            | "tailscale"
            | "immich"
            | "searxng"
    )
}

fn companion_lan_address(app_id: &str) -> Option<String> {
    match app_id {
        "bitcoin" | "bitcoin-knots" | "bitcoin-core" => Some("http://localhost:8334".to_string()),
        "electrumx" | "mempool-electrs" | "electrs" => Some("http://localhost:50002".to_string()),
        _ => None,
    }
}

/// Companion dashboards remain usable while their backend is syncing. Never
/// probe a Bitcoin RPC or Electrum protocol socket as dashboard readiness.
fn package_launch_candidate(
    app_id: &str,
    ports: &[String],
    known: Option<String>,
) -> Option<String> {
    if let Some(companion) = companion_lan_address(app_id) {
        return Some(companion);
    }
    if app_id == "nginx-proxy-manager" {
        // 80/443 serve users' proxy hosts; only container port 81 serves the
        // admin UI. Podman's binding order is unstable across recreation.
        // Resolve its actual host allocation rather than guessing the first
        // HTTP port or hardcoding the default host port 8081.
        let admin_ports: Vec<String> = ports
            .iter()
            .filter(|port| {
                port.split_once("->")
                    .is_some_and(|(_, target)| target == "81/tcp")
            })
            .cloned()
            .collect();
        // With published bindings, a missing admin mapping is not evidence
        // that some unrelated service on the default host port is this UI.
        return extract_lan_address(&admin_ports).or_else(|| {
            if ports.is_empty() {
                known
            } else {
                None
            }
        });
    }
    if uses_allocated_launch_port(app_id) {
        extract_lan_address(ports).or(known)
    } else {
        known.or_else(|| extract_lan_address(ports))
    }
}

fn uses_allocated_launch_port(app_id: &str) -> bool {
    matches!(
        app_id,
        "filebrowser" | "nextcloud" | "nginx-proxy-manager" | "vaultwarden"
    )
}

fn convert_state(container_state: &ContainerState) -> (PackageState, ServiceStatus) {
    match container_state {
        ContainerState::Running => (PackageState::Running, ServiceStatus::Running),
        ContainerState::Stopping => (PackageState::Stopping, ServiceStatus::Stopped),
        ContainerState::Stopped => (PackageState::Stopped, ServiceStatus::Stopped),
        ContainerState::Exited => (PackageState::Exited, ServiceStatus::Stopped),
        ContainerState::Created => (PackageState::Stopped, ServiceStatus::Stopped),
        ContainerState::Paused => (PackageState::Stopped, ServiceStatus::Stopped),
        ContainerState::Unknown(_) => (PackageState::Stopped, ServiceStatus::Stopped),
    }
}

fn package_state_str(state: &PackageState) -> &str {
    match state {
        PackageState::Installing => "installing",
        PackageState::Installed => "installed",
        PackageState::Stopping => "stopping",
        PackageState::Stopped => "stopped",
        PackageState::Exited => "exited",
        PackageState::Starting => "starting",
        PackageState::Running => "running",
        PackageState::Restarting => "restarting",
        PackageState::CreatingBackup => "creating-backup",
        PackageState::RestoringBackup => "restoring-backup",
        PackageState::Removing => "removing",
        PackageState::BackingUp => "backing-up",
        PackageState::Updating => "updating",
    }
}

#[cfg(test)]
mod launch_url_port_tests {
    use super::launch_url_port;

    #[test]
    fn parses_port_with_trailing_path() {
        // Regression: manifest interfaces.main yields a path-suffixed URL.
        // The old rsplit(':') parse produced "8096/" and dropped the URL.
        assert_eq!(launch_url_port("http://localhost:8096/"), Some(8096));
        assert_eq!(launch_url_port("http://localhost:8175/admin"), Some(8175));
    }

    #[test]
    fn parses_bare_authority_port() {
        assert_eq!(launch_url_port("http://localhost:8083"), Some(8083));
    }

    #[test]
    fn rejects_url_without_port() {
        assert_eq!(launch_url_port("http://localhost/"), None);
    }
}

#[cfg(test)]
mod tor_service_name_tests {
    use super::tor_service_name;

    #[test]
    fn bitcoin_core_resolves_to_its_own_hidden_service_dir() {
        // Regression: enrollment (install.rs, tor.create-service) writes
        // HiddenServiceDir/tor-hostnames entries using the raw package_id
        // verbatim, never canonicalized. Aliasing "bitcoin-core" to the
        // shared "bitcoin" name here would point reads at a directory
        // enrollment never creates.
        assert_eq!(tor_service_name("bitcoin-core"), Some("bitcoin-core"));
    }

    #[test]
    fn legacy_bitcoin_ids_share_the_bitcoin_alias() {
        assert_eq!(tor_service_name("bitcoin"), Some("bitcoin"));
        assert_eq!(tor_service_name("bitcoin-knots"), Some("bitcoin"));
        assert_eq!(tor_service_name("bitcoind"), Some("bitcoin"));
    }
}

#[cfg(test)]
mod extract_lan_address_tests {
    use super::{extract_lan_address, package_launch_candidate};

    #[test]
    fn companion_dashboard_wins_over_backend_protocol_ports() {
        for id in ["bitcoin", "bitcoin-core", "bitcoin-knots"] {
            assert_eq!(
                package_launch_candidate(
                    id,
                    &["127.0.0.1:8332->8332/tcp".into()],
                    Some("http://localhost:8332".into())
                )
                .as_deref(),
                Some("http://localhost:8334")
            );
        }
        for id in ["electrumx", "electrs", "mempool-electrs"] {
            assert_eq!(
                package_launch_candidate(
                    id,
                    &["127.0.0.1:50001->50001/tcp".into()],
                    Some("http://localhost:50001".into())
                )
                .as_deref(),
                Some("http://localhost:50002")
            );
        }
        assert_eq!(
            package_launch_candidate(
                "filebrowser",
                &["127.0.0.1:19080->80/tcp".into()],
                Some("http://localhost:8080".into())
            )
            .as_deref(),
            Some("http://localhost:19080")
        );
    }

    #[test]
    fn npm_admin_launch_is_independent_of_proxy_binding_order() {
        let mappings = [
            "10.77.0.2:18081->80/tcp",
            "10.77.0.2:18443->443/tcp",
            "127.0.0.1:8081->81/tcp",
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let ports: Vec<String> = order.iter().map(|&i| mappings[i].into()).collect();
            assert_eq!(
                package_launch_candidate(
                    "nginx-proxy-manager",
                    &ports,
                    Some("http://localhost:8081/".into())
                )
                .as_deref(),
                Some("http://localhost:8081")
            );
        }
    }

    #[test]
    fn npm_admin_launch_respects_host_allocation_and_ipv6_bindings() {
        for binding in ["127.0.0.1", "0.0.0.0", "[::1]", "[::]"] {
            let ports = vec![
                "10.77.0.2:18081->80/tcp".into(),
                format!("{binding}:28081->81/tcp"),
            ];
            assert_eq!(
                package_launch_candidate(
                    "nginx-proxy-manager",
                    &ports,
                    Some("http://localhost:8081/".into())
                )
                .as_deref(),
                Some("http://localhost:28081")
            );
        }
        let proxies = vec![
            "10.77.0.2:18081->80/tcp".into(),
            "10.77.0.2:18443->443/tcp".into(),
        ];
        assert_eq!(
            package_launch_candidate("nginx-proxy-manager", &proxies, None),
            None
        );
        assert_eq!(
            package_launch_candidate(
                "nginx-proxy-manager",
                &proxies,
                Some("http://localhost:8081/".into())
            ),
            None
        );
    }

    #[test]
    fn npm_without_port_information_uses_declared_admin_url() {
        assert_eq!(
            package_launch_candidate(
                "nginx-proxy-manager",
                &[],
                Some("http://localhost:8081/".into())
            )
            .as_deref(),
            Some("http://localhost:8081/")
        );
    }

    #[test]
    fn malformed_published_ports_do_not_become_launch_urls() {
        for port in [
            "81/tcp",
            "127.0.0.1:bad->81/tcp",
            "[::1]:0->81/tcp",
            "[::]:65536->81/tcp",
            "127.0.0.1:8081->81/udp",
        ] {
            assert_eq!(
                package_launch_candidate("nginx-proxy-manager", &[port.into()], None),
                None
            );
        }
    }

    #[test]
    fn skips_ssh_port_when_web_port_is_published() {
        // gitea: SSH published before the web port, in podman's list order.
        let ports = vec![
            "0.0.0.0:2222->22/tcp".to_string(),
            "0.0.0.0:3001->3000/tcp".to_string(),
        ];
        assert_eq!(
            extract_lan_address(&ports).as_deref(),
            Some("http://localhost:3001")
        );
    }

    #[test]
    fn falls_back_to_first_port_when_nothing_looks_like_http() {
        let ports = vec!["0.0.0.0:2222->22/tcp".to_string()];
        assert_eq!(
            extract_lan_address(&ports).as_deref(),
            Some("http://localhost:2222")
        );
    }

    #[test]
    fn single_http_port_still_resolves() {
        let ports = vec!["0.0.0.0:8096->8096/tcp".to_string()];
        assert_eq!(
            extract_lan_address(&ports).as_deref(),
            Some("http://localhost:8096")
        );
    }

    #[test]
    fn handles_port_ranges() {
        let ports = vec!["0.0.0.0:18443-18444->18443-18444/tcp".to_string()];
        assert_eq!(
            extract_lan_address(&ports).as_deref(),
            Some("http://localhost:18443")
        );
    }

    #[test]
    fn no_ports_returns_none() {
        let ports: Vec<String> = vec![];
        assert_eq!(extract_lan_address(&ports), None);
    }
}
