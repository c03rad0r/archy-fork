// Docker Package Scanner
// Scans docker-compose containers and converts them to package data

use anyhow::Result;
use archipelago_container::{
    ContainerRuntime as ContainerRuntimeTrait, ContainerState, PodmanClient,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

use super::image_versions;
use crate::data_model::{
    Description, InstalledPackageDataEntry, InterfaceAddress, Interfaces, MainInterface, Manifest,
    PackageDataEntry, PackageState, ServiceStatus, StaticFiles,
};

pub struct DockerPackageScanner {
    runtime: Arc<dyn ContainerRuntimeTrait>,
}

impl DockerPackageScanner {
    pub fn new(runtime: Arc<dyn ContainerRuntimeTrait>) -> Self {
        Self { runtime }
    }

    /// Scan Docker containers and convert to package data
    pub async fn scan_containers(&self) -> Result<HashMap<String, PackageDataEntry>> {
        let containers = self.runtime.list_containers().await?;

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
            // Extract app ID from container name
            // Support both archy-* containers (docker-compose) and plain names (manual)
            let app_id = if container.name.starts_with("archy-") {
                container
                    .name
                    .strip_prefix("archy-")
                    .unwrap_or(&container.name)
                    .to_string()
            } else {
                // Use the container name as-is for manually started containers
                container.name.clone()
            };

            // Normalize multi-container app IDs to their canonical names
            let app_id = match app_id.as_str() {
                "immich_server" => "immich".to_string(),
                _ => app_id,
            };

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
                let candidate = if uses_allocated_launch_port(&app_id) {
                    extract_lan_address(&container.ports)
                        .or_else(|| PodmanClient::lan_address_for(&app_id))
                } else {
                    PodmanClient::lan_address_for(&app_id)
                        .or_else(|| extract_lan_address(&container.ports))
                };
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
            let running_version = image_versions::extract_version_from_image(&container.image);

            // Decoupled from the binary OTA: prefer the remote app catalog,
            // falling back to the image-versions.sh pin when uncovered/offline.
            let available_update =
                crate::container::app_catalog::available_update_for_app(&app_id, &container.image);

            let package = PackageDataEntry {
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

        Ok(packages)
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
fn tor_service_name(app_id: &str) -> Option<&'static str> {
    match app_id {
        "archipelago" => Some("archipelago"),
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" | "bitcoind" => Some("bitcoin"),
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
        let Some(public_part) = port_str.split("->").next() else {
            continue;
        };
        let Some(port_part) = public_part.split(':').nth(1) else {
            continue;
        };
        // Extract just the first port if it's a range (e.g., "18443-18444" -> "18443")
        let host_port = port_part.split('-').next().unwrap_or(port_part);
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
mod extract_lan_address_tests {
    use super::extract_lan_address;

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
