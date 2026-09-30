//! Podman container management via the REST API unix socket.
//!
//! Connects to the rootless Podman API at /run/user/{UID}/podman/podman.sock.
//! All operations are non-blocking async via tokio + hyper.
//! Falls back to CLI only for image pulls (long-running streaming operations).

use crate::manifest::AppManifest;
use anyhow::{Context, Result};
use hyper::{Body, Request, Uri};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::net::UnixStream;

const API_VERSION: &str = "v4.0.0";
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const LONG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Error)]
pub enum PodmanError {
    #[error("Podman API error: {0}")]
    ApiError(String),
    #[error("Container not found: {0}")]
    NotFound(String),
    #[error("Podman socket not available: {0}")]
    SocketUnavailable(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStatus {
    pub id: String,
    pub name: String,
    pub state: ContainerState,
    pub health: Option<String>,
    pub exit_code: Option<i32>,
    pub started_at: Option<String>,
    pub image: String,
    pub created: String,
    pub ports: Vec<String>,
    pub lan_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContainerState {
    Created,
    Running,
    Stopping,
    Stopped,
    Exited,
    Paused,
    Unknown(String),
}

impl From<&str> for ContainerState {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "created" | "initialized" => ContainerState::Created,
            "running" => ContainerState::Running,
            "stopping" | "removing" => ContainerState::Stopping,
            "stopped" => ContainerState::Stopped,
            "exited" => ContainerState::Exited,
            "paused" => ContainerState::Paused,
            other => ContainerState::Unknown(other.to_string()),
        }
    }
}

/// Parse health status from podman's Status string (e.g., "Up 5 minutes (healthy)")
fn parse_health_from_status(status: &str) -> Option<String> {
    if let Some(start) = status.rfind('(') {
        if let Some(end) = status.rfind(')') {
            if start < end {
                return Some(status[start + 1..end].to_string());
            }
        }
    }
    None
}

pub struct PodmanClient {
    socket_path: PathBuf,
}

impl PodmanClient {
    pub fn new(user: String) -> Self {
        // Determine socket path based on user
        let uid = Self::get_uid(&user);
        let socket_path = PathBuf::from(format!("/run/user/{}/podman/podman.sock", uid));
        Self { socket_path }
    }

    fn get_uid(user: &str) -> u32 {
        // Try to get UID from /etc/passwd
        if let Ok(content) = std::fs::read_to_string("/etc/passwd") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 3 && parts[0] == user {
                    if let Ok(uid) = parts[2].parse() {
                        return uid;
                    }
                }
            }
        }
        // Default to 1000 (standard first user)
        1000
    }

    /// Map container name to its UI launch URL
    pub fn lan_address_for(name: &str) -> Option<String> {
        if let Some(url) = manifest_lan_address_for(name) {
            return Some(url);
        }

        let url = match name {
            "bitcoin-knots" | "bitcoin-ui" => "http://localhost:8334",
            "lnd" | "archy-lnd-ui" => "http://localhost:18083",
            "archy-mempool-web" | "mempool" => "http://localhost:4080",
            "ollama" => "http://localhost:11434",
            "cryptpad" => "http://localhost:3003",
            "penpot" => "http://localhost:9001",
            "immich_server" | "immich" => "http://localhost:2283",
            // Gitea publishes SSH (2222) and web (3001). Without a manifest on
            // disk, extract_lan_address() returns whichever podman lists first —
            // which can be the SSH port, breaking the launch. Pin the web UI.
            "gitea" => "http://localhost:3001",
            "nginx-proxy-manager" => "http://localhost:8081",
            "fedimint-gateway" => "http://localhost:8176",
            "endurain" => "http://localhost:8080",
            // HTTPS: netbird's dashboard needs a secure context for OIDC PKCE
            // (window.crypto.subtle), so the proxy serves TLS on 8087 (issue #15).
            "netbird" => "https://localhost:8087",
            "electrs" | "archy-electrs-ui" => "http://localhost:50002",
            _ => return None,
        };
        Some(url.to_string())
    }

    // ─── API Client ──────────────────────────────────────────────

    /// Send a request to the Podman API via unix socket.
    async fn api_request(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value> {
        let socket_path = self.socket_path.clone();

        // Connect to the unix socket (30s timeout — podman can be slow under load on boot)
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            UnixStream::connect(&socket_path),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Podman socket connection timed out (30s)"))?
        .context(format!(
            "Cannot connect to Podman socket at {}",
            socket_path.display()
        ))?;

        // Build the hyper client with the unix stream
        let (mut sender, conn) = hyper::client::conn::Builder::new()
            .handshake::<_, Body>(stream)
            .await
            .context("Podman API handshake failed")?;

        // Spawn the connection handler
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!("Podman API connection ended: {}", e);
            }
        });

        // Build the request
        let uri: Uri = format!("/{}/{}", API_VERSION, path.trim_start_matches('/'))
            .parse()
            .context("Invalid API path")?;

        let req = match method {
            "POST" => {
                let body_str = match body {
                    Some(b) => serde_json::to_string(&b)
                        .context("Failed to serialize request body to JSON")?,
                    None => String::new(),
                };
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("Host", "localhost")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body_str))
                    .context("Failed to build POST request")?
            }
            "DELETE" => Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("Host", "localhost")
                .body(Body::empty())
                .context("Failed to build DELETE request")?,
            _ => Request::builder()
                .method("GET")
                .uri(uri)
                .header("Host", "localhost")
                .body(Body::empty())
                .context("Failed to build GET request")?,
        };

        // Send with timeout
        let resp = tokio::time::timeout(timeout, sender.send_request(req))
            .await
            .map_err(|_| {
                anyhow::anyhow!("Podman API request timed out after {}s", timeout.as_secs())
            })?
            .context("Podman API request failed")?;

        let status = resp.status();
        let body_bytes = hyper::body::to_bytes(resp.into_body())
            .await
            .context("Failed to read Podman API response")?;

        if status == hyper::StatusCode::NOT_FOUND {
            return Err(anyhow::anyhow!("Not found"));
        }

        if !status.is_success() {
            let error_text = String::from_utf8_lossy(&body_bytes);
            return Err(anyhow::anyhow!(
                "Podman API {} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                error_text
            ));
        }

        // Some endpoints return empty body on success (start/stop/restart)
        if body_bytes.is_empty() {
            return Ok(serde_json::json!({"ok": true}));
        }

        serde_json::from_slice(&body_bytes).context("Failed to parse Podman API JSON response")
    }

    /// Simple POST with no body (start/stop/restart)
    async fn api_post_action(&self, path: &str) -> Result<()> {
        self.api_request("POST", path, None, DEFAULT_TIMEOUT)
            .await?;
        Ok(())
    }

    // ─── Container Operations ────────────────────────────────────

    pub async fn pull_image(&self, image: &str, signature: Option<&str>) -> Result<()> {
        // A declared (non-placeholder) signature must verify before we fetch
        // anything; placeholder/absent claims pull unverified until the
        // signing ceremony ships real signatures (see image_verify).
        crate::image_verify::enforce_signature_claim(
            image,
            signature,
            image_uses_insecure_registry(image),
        )
        .await?;

        // Stall-aware + retried, mirroring the legacy installer's
        // pull_one_url_with_progress. The old single-attempt hard 600s wall
        // clock killed slow-but-progressing pulls, which every orchestrator
        // stack (btcpay, indeedhub, …) surfaced as "first install fails,
        // second succeeds" once the layer cache was warm. Podman keeps
        // completed layers between attempts, so retries resume cheaply.
        const MAX_ATTEMPTS: u32 = 3;
        const BACKOFF_SECS: [u64; 2] = [5, 15];

        let mut last_err = anyhow::anyhow!("podman pull {image}: no attempt ran");
        for attempt in 1..=MAX_ATTEMPTS {
            match pull_image_stall_aware(image).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(
                        "Image pull failed for {} (attempt {}/{}): {:#}",
                        image,
                        attempt,
                        MAX_ATTEMPTS,
                        e
                    );
                    last_err = e;
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_secs(
                            BACKOFF_SECS[(attempt - 1) as usize],
                        ))
                        .await;
                    }
                }
            }
        }
        Err(last_err)
    }

    pub async fn create_container(&self, manifest: &AppManifest, name: &str) -> Result<String> {
        // Build the container spec for the API
        let mut port_mappings = Vec::new();
        for port in &manifest.app.ports {
            if !crate::manifest::host_can_bind_publish_ip(&port.bind) {
                tracing::warn!(
                    app = %manifest.app.id,
                    bind = %port.bind,
                    host_port = port.host,
                    "dropping publish: bind address not assignable on this host"
                );
                continue;
            }
            port_mappings.push(podman_publish_mapping(port));
        }

        let mut mounts = Vec::new();
        let mut named_volumes = Vec::new();
        for volume in &manifest.app.volumes {
            if volume.volume_type == "tmpfs" {
                let options: Vec<String> = volume
                    .tmpfs_options
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                mounts.push(serde_json::json!({
                    "destination": volume.target,
                    "type": "tmpfs",
                    "options": options,
                }));
            } else if volume.volume_type == "volume" {
                // Named podman volume. The libpod create spec carries these in
                // the separate `volumes` field ({Name, Dest, Options}), NOT in
                // `mounts`: sending one as a bind mount makes the API treat
                // the bare volume name as a host path and the create fails —
                // which left indeedhub-postgres/-minio permanently absent on
                // legacy-path nodes (the reconciler removed the old container
                // for drift, then could never create its replacement).
                named_volumes.push(serde_json::json!({
                    "Name": volume.source,
                    "Dest": volume.target,
                    "Options": volume.options,
                }));
            } else {
                mounts.push(serde_json::json!({
                    "destination": volume.target,
                    "source": volume.source,
                    "type": "bind",
                    "options": volume.options,
                }));
            }
        }

        let mut env_map = serde_json::Map::new();
        for env in &manifest.app.environment {
            if let Some((k, v)) = env.split_once('=') {
                env_map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
            }
        }

        let cap_add: Vec<String> = manifest.app.security.capabilities.clone();
        let cap_drop = vec!["ALL".to_string()];

        let image_ref = manifest.app.container.image_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "container config for {} has neither a valid image nor build source",
                manifest.app.id
            )
        })?;

        // Build resource_limits conditionally: if the manifest has no memory or
        // cpu limit, OMIT the field entirely rather than sending 0. The podman
        // libpod HTTP API treats `memory.limit: 0` as "set MemoryMax=0" which
        // systemd then rejects at container-start time. Absent = unlimited.
        let mut resource_limits = serde_json::Map::new();
        if let Some(mem_bytes) = manifest
            .app
            .resources
            .memory_limit
            .as_ref()
            .and_then(|m| parse_memory_limit(m))
        {
            resource_limits.insert(
                "memory".to_string(),
                serde_json::json!({ "limit": mem_bytes }),
            );
        }
        if let Some(cpu) = manifest.app.resources.cpu_limit {
            resource_limits.insert(
                "cpu".to_string(),
                serde_json::json!({
                    "quota": (cpu as i64) * 100_000,
                    "period": 100_000u64,
                }),
            );
        }

        let (net_mode, custom_network) = podman_network_settings(
            manifest.app.container.network.as_deref(),
            manifest.app.security.network_policy.as_str(),
        );

        // Secret env travels by reference (podman injects the value at
        // start), so it never shows up in `podman inspect` output. The
        // combined content hash rides as a label for rotation-drift checks.
        let mut secret_env_map = serde_json::Map::new();
        for r in &manifest.app.container.secret_env_refs {
            secret_env_map.insert(
                r.env_key.clone(),
                serde_json::Value::String(r.secret_name.clone()),
            );
        }
        let mut labels_map = serde_json::Map::new();
        if let Some(hash) = &manifest.app.container.secret_env_hash {
            labels_map.insert(
                crate::manifest::SECRET_ENV_HASH_LABEL.to_string(),
                serde_json::Value::String(hash.clone()),
            );
        }

        let mut body = serde_json::json!({
            "name": name,
            "image": image_ref,
            "portmappings": port_mappings,
            "mounts": mounts,
            "volumes": named_volumes,
            "env": env_map,
            "secret_env": secret_env_map,
            "labels": labels_map,
            "entrypoint": manifest.app.container.entrypoint.clone(),
            "command": manifest.app.container.custom_args.clone(),
            "hostadd": [
                "host.containers.internal:host-gateway",
                "host.archipelago:10.89.0.1",
            ],
            "devices": manifest.app.devices.iter().map(|d| {
                serde_json::json!({"path": d})
            }).collect::<Vec<_>>(),
            "resource_limits": resource_limits,
            "cap_add": cap_add,
            "cap_drop": cap_drop,
            "read_only_filesystem": manifest.app.security.readonly_root,
            "no_new_privileges": manifest.app.security.no_new_privileges,
            "restart_policy": "unless-stopped",
            "restart_tries": 5,
            "netns": {
                "nsmode": net_mode
            },
        });
        if let Some(network) = custom_network {
            // The container always answers to its own name; manifest
            // network_aliases add extra short hostnames peers may bake in
            // (e.g. indeedhub's api/minio/relay). Dedup so a manifest that
            // redundantly lists its own name doesn't double it.
            let mut aliases = vec![name.to_string()];
            for a in &manifest.app.container.network_aliases {
                if !aliases.iter().any(|x| x == a) {
                    aliases.push(a.clone());
                }
            }
            body.as_object_mut()
                .expect("container create body is a JSON object")
                .insert(
                    "networks".to_string(),
                    serde_json::json!({ network: { "aliases": aliases } }),
                );
        }

        let result = self
            .api_request("POST", "libpod/containers/create", Some(body), LONG_TIMEOUT)
            .await?;

        let id = result["Id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .context("Podman API returned no container ID — creation may have failed")?;

        Ok(id)
    }

    pub async fn start_container(&self, name: &str) -> Result<()> {
        self.api_post_action(&format!("libpod/containers/{}/start", name))
            .await
    }

    pub async fn stop_container(&self, name: &str) -> Result<()> {
        self.stop_container_with_grace(name, 10).await
    }

    /// Stop via libpod honouring a per-app grace (seconds). The HTTP deadline is
    /// kept above the grace so the post-grace SIGKILL lands before we give up —
    /// otherwise slow-to-SIGTERM apps (fedimint, bitcoin-core, electrumx…) time
    /// out at exactly the grace boundary and the stop is reported as failed.
    pub async fn stop_container_with_grace(&self, name: &str, grace_secs: u64) -> Result<()> {
        let deadline = std::time::Duration::from_secs(
            grace_secs + crate::runtime::STOP_GRACE_DEADLINE_BUFFER_SECS,
        );
        self.api_request(
            "POST",
            &format!("libpod/containers/{}/stop?t={}", name, grace_secs),
            None,
            deadline,
        )
        .await
        .map(|_| ())
    }

    pub async fn restart_container(&self, name: &str) -> Result<()> {
        self.api_request(
            "POST",
            &format!("libpod/containers/{}/restart?t=10", name),
            None,
            DEFAULT_TIMEOUT,
        )
        .await
        .map(|_| ())
    }

    pub async fn remove_container(&self, name: &str) -> Result<()> {
        self.api_request(
            "DELETE",
            &format!("libpod/containers/{}?force=true", name),
            None,
            DEFAULT_TIMEOUT,
        )
        .await
        .map(|_| ())
    }

    pub async fn get_container_status(&self, name: &str) -> Result<ContainerStatus> {
        let data = self
            .api_request(
                "GET",
                &format!("libpod/containers/{}/json", name),
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;

        let state_str = data["State"]["Status"].as_str().unwrap_or("unknown");
        let health = data["State"]["Health"]["Status"]
            .as_str()
            .map(|s| s.to_string());
        let started_at = data["State"]["StartedAt"].as_str().map(|s| s.to_string());
        let container_name = data["Name"].as_str().unwrap_or(name).to_string();

        // Parse port bindings
        let ports = parse_port_bindings(&data["HostConfig"]["PortBindings"]);
        let lan_address = Self::lan_address_for(&container_name);

        let exit_code = data["State"]["ExitCode"].as_i64().map(|c| c as i32);

        Ok(ContainerStatus {
            id: data["Id"].as_str().unwrap_or("").to_string(),
            name: container_name,
            state: ContainerState::from(state_str),
            health,
            exit_code,
            started_at,
            image: data["ImageName"]
                .as_str()
                .or_else(|| data["Config"]["Image"].as_str())
                .unwrap_or("")
                .to_string(),
            created: data["Created"].as_str().unwrap_or("").to_string(),
            ports,
            lan_address,
        })
    }

    pub async fn get_container_logs(&self, name: &str, lines: u32) -> Result<Vec<String>> {
        // Logs endpoint returns raw text, not JSON — use CLI for this
        let mut cmd = tokio::process::Command::new("podman");
        cmd.arg("logs")
            .arg("--tail")
            .arg(lines.to_string())
            .arg(name);

        let output = tokio::time::timeout(DEFAULT_TIMEOUT, cmd.output())
            .await
            .map_err(|_| anyhow::anyhow!("Container logs timed out"))?
            .context("Failed to get container logs")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to get logs: {}", stderr));
        }

        // Podman logs go to both stdout and stderr
        let mut all_output = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            all_output.push_str(&stderr);
        }
        Ok(all_output.lines().map(|s| s.to_string()).collect())
    }

    pub async fn list_containers(&self) -> Result<Vec<ContainerStatus>> {
        let data = self
            .api_request(
                "GET",
                "libpod/containers/json?all=true",
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;

        let containers = data
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Expected array from containers/json"))?;

        let mut result = Vec::with_capacity(containers.len());

        for c in containers {
            let name = if let Some(names) = c["Names"].as_array() {
                names
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                c["Names"].as_str().unwrap_or("").to_string()
            };

            let ports = if let Some(ports_array) = c["Ports"].as_array() {
                ports_array
                    .iter()
                    .filter_map(|port| {
                        let host_port = port["host_port"].as_u64()?;
                        let container_port = port["container_port"].as_u64()?;
                        let protocol = port["protocol"].as_str().unwrap_or("tcp");
                        Some(format!(
                            "0.0.0.0:{}->{}/{}",
                            host_port, container_port, protocol
                        ))
                    })
                    .collect()
            } else {
                vec![]
            };

            let status_str = c["Status"].as_str().unwrap_or("");
            let health = parse_health_from_status(status_str)
                .or_else(|| c["Health"].as_str().map(|s| s.to_string()));
            let started_at = c["StartedAt"]
                .as_str()
                .or_else(|| c["Started"].as_str())
                .map(|s| s.to_string());
            let lan_address = Self::lan_address_for(&name);

            let exit_code = c["ExitCode"]
                .as_i64()
                .or_else(|| c["State"]["ExitCode"].as_i64())
                .map(|c| c as i32);

            result.push(ContainerStatus {
                id: c["Id"].as_str().unwrap_or("").to_string(),
                name,
                state: ContainerState::from(c["State"].as_str().unwrap_or("unknown")),
                health,
                exit_code,
                started_at,
                image: c["Image"].as_str().unwrap_or("").to_string(),
                created: c["Created"].as_str().unwrap_or("").to_string(),
                ports,
                lan_address,
            });
        }

        Ok(result)
    }

    /// Check if the Podman socket is available and responding.
    pub async fn health_check(&self) -> bool {
        self.api_request(
            "GET",
            "libpod/info",
            None,
            std::time::Duration::from_secs(5),
        )
        .await
        .is_ok()
    }
}

/// Registries we ship with as `--tls-verify=false` because they're internal
/// HTTP mirrors. Add a host:port here only if it's a controlled mirror that
/// the fleet trusts and operators won't ever paste a malicious URL into.
const INSECURE_REGISTRY_HOSTS: &[&str] = &["source.archipelago-foundation.org"];

pub fn image_uses_insecure_registry(image: &str) -> bool {
    image
        .split('/')
        .next()
        .is_some_and(|host| INSECURE_REGISTRY_HOSTS.contains(&host))
}

// Keep the explicitly declared bind and transport identical to Quadlet. The
// app gate owns external listeners; container publication must not bypass it.
fn podman_publish_mapping(port: &crate::manifest::PortMapping) -> serde_json::Value {
    let protocol = match port.protocol.to_ascii_lowercase().as_str() {
        "udp" => "udp",
        "sctp" => "sctp",
        _ => "tcp",
    };
    let mut mapping = serde_json::json!({
        "container_port": port.container,
        "host_port": port.host,
        "protocol": protocol,
    });
    if !port.bind.is_empty() {
        mapping["host_ip"] = serde_json::json!(port.bind);
    }
    mapping
}

fn podman_network_settings(
    network: Option<&str>,
    network_policy: &str,
) -> (&'static str, Option<String>) {
    match network {
        Some("") => ("bridge", None),
        Some("host") => ("host", None),
        Some("bridge") => ("bridge", None),
        Some("none") => ("none", None),
        Some("slirp4netns") => ("slirp4netns", None),
        Some("pasta") => ("pasta", None),
        Some("private") => ("private", None),
        Some(custom) => ("bridge", Some(custom.to_string())),
        None if network_policy == "host" => ("host", None),
        None => ("bridge", None),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────

/// One `podman pull` attempt with a stall-aware budget instead of a hard
/// wall clock. A pull is only killed when NOTHING is observably happening —
/// no stderr output AND no byte growth in podman's TMPDIR staging dir — for
/// PULL_STALL_TIMEOUT_SECS, or at a generous absolute ceiling. Dead
/// registries still fail fast (no bytes ever land, so the 3-minute stall
/// window is the effective bound), while a slow-but-moving multi-GB pull is
/// left alone. Same design as the legacy installer's
/// pull_one_url_with_progress (LND "first install fails" fix).
async fn pull_image_stall_aware(image: &str) -> Result<()> {
    const PULL_STALL_TIMEOUT_SECS: u64 = 180;
    const PULL_MAX_SECS: u64 = 1800;
    const PULL_POLL_INTERVAL_SECS: u64 = 5;

    // Rootless podman's user namespace makes /var/tmp read-only; stage into
    // the user's containers tmp (same dir every other pull path uses) — it
    // doubles as the "bytes are moving" signal for stall detection.
    let user_tmp = format!(
        "{}/.local/share/containers/tmp",
        std::env::var("HOME").unwrap_or_else(|_| "/home/archipelago".to_string())
    );
    let _ = std::fs::create_dir_all(&user_tmp);

    let mut cmd = tokio::process::Command::new("podman");
    cmd.arg("pull");
    if image_uses_insecure_registry(image) {
        cmd.arg("--tls-verify=false");
    }
    cmd.arg(image);
    cmd.env("TMPDIR", &user_tmp);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);
    let mut child = cmd.spawn().context("Failed to start podman pull")?;

    let started = std::time::Instant::now();
    // Seconds-since-start of the last stderr line, updated by the reader
    // task. Starts at 0 so the stall window opens at spawn.
    let last_line_at = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    // Ring of recent stderr lines so a failed pull reports podman's actual
    // error, not just "exit status 125".
    let recent_stderr = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::<
        String,
    >::with_capacity(8)));
    if let Some(stderr) = child.stderr.take() {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        let line_clock = std::sync::Arc::clone(&last_line_at);
        let stderr_ring = std::sync::Arc::clone(&recent_stderr);
        let started_reader = started;
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                line_clock.store(
                    started_reader.elapsed().as_secs(),
                    std::sync::atomic::Ordering::Relaxed,
                );
                if let Ok(mut ring) = stderr_ring.lock() {
                    if ring.len() >= 8 {
                        ring.pop_front();
                    }
                    ring.push_back(line);
                }
            }
        });
    }

    let stderr_tail = |ring: &std::sync::Mutex<std::collections::VecDeque<String>>| {
        ring.lock()
            .map(|r| r.iter().cloned().collect::<Vec<_>>().join(" | "))
            .unwrap_or_default()
    };

    let mut last_staged_bytes = dir_size_bytes(&user_tmp);
    let mut last_staged_change = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                anyhow::bail!(
                    "podman pull {image} failed ({status}): {}",
                    stderr_tail(&recent_stderr)
                );
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(anyhow::anyhow!("podman pull {image} process error: {e}"));
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(PULL_POLL_INTERVAL_SECS)).await;

        if started.elapsed() > std::time::Duration::from_secs(PULL_MAX_SECS) {
            let _ = child.kill().await;
            let _ = child.wait().await; // reap zombie
            anyhow::bail!("podman pull {image} exceeded absolute {PULL_MAX_SECS}s ceiling");
        }

        // Activity signal 1: stderr output from podman.
        let line_age = started
            .elapsed()
            .as_secs()
            .saturating_sub(last_line_at.load(std::sync::atomic::Ordering::Relaxed));
        // Activity signal 2: staged layer bytes growing in TMPDIR.
        let staged = dir_size_bytes(&user_tmp);
        if staged != last_staged_bytes {
            last_staged_bytes = staged;
            last_staged_change = std::time::Instant::now();
        }

        if line_age > PULL_STALL_TIMEOUT_SECS
            && last_staged_change.elapsed()
                > std::time::Duration::from_secs(PULL_STALL_TIMEOUT_SECS)
        {
            let _ = child.kill().await;
            let _ = child.wait().await; // reap zombie
            anyhow::bail!(
                "podman pull {image} stalled ({PULL_STALL_TIMEOUT_SECS}s with no output and no staged bytes): {}",
                stderr_tail(&recent_stderr)
            );
        }
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

fn parse_port_bindings(bindings: &serde_json::Value) -> Vec<String> {
    let mut ports = Vec::new();
    if let Some(obj) = bindings.as_object() {
        for (container_port, host_bindings) in obj {
            if let Some(arr) = host_bindings.as_array() {
                for binding in arr {
                    let host_ip = binding["HostIp"].as_str().unwrap_or("0.0.0.0");
                    let host_port = binding["HostPort"].as_str().unwrap_or("");
                    if !host_port.is_empty() {
                        ports.push(format!("{}:{}->{}", host_ip, host_port, container_port));
                    }
                }
            }
        }
    }
    ports
}

fn manifest_lan_address_for(container_name: &str) -> Option<String> {
    for apps_dir in manifest_apps_dirs() {
        let Ok(entries) = std::fs::read_dir(apps_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path().join("manifest.yml");
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(manifest) = AppManifest::parse(&contents) else {
                continue;
            };
            if manifest_runtime_names(&manifest)
                .iter()
                .any(|name| name == container_name)
            {
                if let Some(url) = manifest_primary_interface_url(&manifest) {
                    return Some(url);
                }
                if manifest_has_http_health(&manifest) {
                    if let Some(port) = manifest
                        .app
                        .ports
                        .iter()
                        .find(|port| port.protocol.eq_ignore_ascii_case("tcp"))
                        .map(|port| port.host)
                    {
                        return Some(format!("http://localhost:{port}"));
                    }
                }
            }
        }
    }
    None
}

fn manifest_primary_interface_url(manifest: &AppManifest) -> Option<String> {
    let main = manifest.app.interfaces.get("main")?;
    if main.interface_type != "ui" {
        return None;
    }
    Some(format!(
        "{}://localhost:{}{}",
        main.protocol, main.port, main.path
    ))
}

fn manifest_has_http_health(manifest: &AppManifest) -> bool {
    manifest
        .app
        .health_check
        .as_ref()
        .is_some_and(|health| health.check_type.eq_ignore_ascii_case("http"))
}

fn manifest_runtime_names(manifest: &AppManifest) -> Vec<String> {
    let mut names = vec![manifest_container_name(manifest)];
    match manifest.app.id.as_str() {
        "bitcoin-ui" | "electrs-ui" | "lnd-ui" => names.push(manifest.app.id.clone()),
        "fedimint" => names.push("fedimintd".to_string()),
        "immich" => names.push("immich_server".to_string()),
        _ => {}
    }
    names
}

fn manifest_container_name(manifest: &AppManifest) -> String {
    if let Some(v) = manifest.app.extensions.get("container_name") {
        if let Some(s) = v.as_str() {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    match manifest.app.id.as_str() {
        "bitcoin-ui" | "electrs-ui" | "lnd-ui" => format!("archy-{}", manifest.app.id),
        id => id.to_string(),
    }
}

fn manifest_apps_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        dirs.push(Path::new(&manifest_dir).join("../../apps"));
    }
    dirs.extend([
        Path::new("apps").to_path_buf(),
        Path::new("/opt/archipelago/apps").to_path_buf(),
        Path::new("/opt/archipelago/web-ui/archipelago-runtime/apps").to_path_buf(),
    ]);
    dirs
}

fn parse_memory_limit(limit: &str) -> Option<i64> {
    // Supports the Kubernetes-style suffixes used throughout apps/*/manifest.yml
    // (IEC binary: Ki/Mi/Gi/Ti) as well as the shorter docker-style k/m/g/t.
    // Longest suffix matched first so "Mi" isn't mis-matched as "m".
    //
    // Historical bug: we used to lowercase+trim_end_matches('m'), which turned
    // "128Mi" into "128i" → parse::<f64> failed → None → .unwrap_or(0) wrote
    // memory.limit:0 into the OCI spec, which systemd then rejected at start
    // time with "MemoryMax is out of range" on rootless podman. See
    // docs/rust-orchestrator-migration.md Step 9 notes.
    let trimmed = limit.trim();
    if trimmed.is_empty() {
        return None;
    }
    const UNITS: &[(&str, i64)] = &[
        ("Ki", 1024),
        ("Mi", 1024 * 1024),
        ("Gi", 1024 * 1024 * 1024),
        ("Ti", 1024i64 * 1024 * 1024 * 1024),
        ("kB", 1000),
        ("MB", 1_000_000),
        ("GB", 1_000_000_000),
        ("TB", 1_000_000_000_000),
        ("k", 1024),
        ("K", 1024),
        ("m", 1024 * 1024),
        ("M", 1024 * 1024),
        ("g", 1024 * 1024 * 1024),
        ("G", 1024 * 1024 * 1024),
        ("t", 1024i64 * 1024 * 1024 * 1024),
        ("T", 1024i64 * 1024 * 1024 * 1024),
        ("b", 1),
        ("B", 1),
    ];
    for (suffix, multiplier) in UNITS {
        if let Some(num) = trimmed.strip_suffix(suffix) {
            let num = num.trim();
            return num
                .parse::<f64>()
                .ok()
                .map(|v| (v * (*multiplier as f64)) as i64)
                .filter(|n| *n > 0);
        }
    }
    // No recognised suffix — treat as raw bytes.
    trimmed.parse::<i64>().ok().filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insecure_registry_detection_matches_http_mirrors_only() {
        assert!(image_uses_insecure_registry(
            "source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest"
        ));
        // The legacy Hetzner mirror at 23.182.128.160 was decommissioned and
        // is no longer trusted — it must NOT bypass TLS even if a stale
        // registry config still references it.
        assert!(!image_uses_insecure_registry(
            "23.182.128.160:3000/lfg2025/filebrowser:v2.27.0"
        ));
        // HTTPS registries never match the insecure list.
        assert!(!image_uses_insecure_registry(
            "ghcr.io/lfg2025/bitcoin-knots:latest"
        ));
        assert!(!image_uses_insecure_registry(
            "docker.io/library/nginx:latest"
        ));
        // Spoofing immune: an attacker host that prefixes the trusted IP
        // string into its own URL still has the attacker host in the
        // registry-host slot, so it does NOT match.
        assert!(!image_uses_insecure_registry(
            "evil.example:80/source.archipelago-foundation.org/lfg2025/x:latest"
        ));
    }

    #[test]
    fn portainer_manifest_keeps_private_network_and_loopback_api_publication() {
        let m = AppManifest::parse(include_str!("../../../apps/portainer/manifest.yml")).unwrap();
        assert_eq!(
            podman_network_settings(
                m.app.container.network.as_deref(),
                &m.app.security.network_policy
            ),
            ("slirp4netns", None)
        );
        assert_eq!(
            podman_publish_mapping(&m.app.ports[0]),
            serde_json::json!({
                "container_port": 9000, "host_port": 9000, "protocol": "tcp", "host_ip": "127.0.0.1"
            })
        );
    }

    #[test]
    fn podman_network_settings_uses_networks_map_for_custom_networks() {
        assert_eq!(
            podman_network_settings(Some("archy-net"), "isolated"),
            ("bridge", Some("archy-net".to_string()))
        );
        assert_eq!(
            podman_network_settings(Some("host"), "isolated"),
            ("host", None)
        );
        assert_eq!(
            podman_network_settings(Some(""), "isolated"),
            ("bridge", None)
        );
        assert_eq!(podman_network_settings(None, "host"), ("host", None));
        assert_eq!(podman_network_settings(None, "isolated"), ("bridge", None));
    }

    #[test]
    fn lan_address_uses_manifest_http_port_for_regular_apps() {
        assert_eq!(
            PodmanClient::lan_address_for("filebrowser").as_deref(),
            Some("http://localhost:8083")
        );
    }

    #[test]
    fn lan_address_prefers_manifest_main_interface() {
        assert_eq!(
            PodmanClient::lan_address_for("fedimint").as_deref(),
            Some("http://localhost:8175/")
        );
    }

    #[test]
    fn lan_address_does_not_expose_tcp_only_service_ports() {
        assert_eq!(
            PodmanClient::lan_address_for("bitcoin-knots").as_deref(),
            Some("http://localhost:8334")
        );
    }

    #[test]
    fn parse_memory_limit_iec_binary_suffixes() {
        // Kubernetes-style — this is what apps/*/manifest.yml uses.
        assert_eq!(parse_memory_limit("128Mi"), Some(128 * 1024 * 1024));
        assert_eq!(parse_memory_limit("64Mi"), Some(64 * 1024 * 1024));
        assert_eq!(parse_memory_limit("4Gi"), Some(4i64 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_limit("512Ki"), Some(512 * 1024));
    }

    #[test]
    fn parse_memory_limit_shorthand_suffixes() {
        // Docker-style shorthand — treated as IEC binary for backwards compat.
        assert_eq!(parse_memory_limit("128m"), Some(128 * 1024 * 1024));
        assert_eq!(parse_memory_limit("128M"), Some(128 * 1024 * 1024));
        assert_eq!(parse_memory_limit("2g"), Some(2i64 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_limit("2G"), Some(2i64 * 1024 * 1024 * 1024));
    }

    #[test]
    fn parse_memory_limit_si_decimal_suffixes() {
        assert_eq!(parse_memory_limit("1MB"), Some(1_000_000));
        assert_eq!(parse_memory_limit("1GB"), Some(1_000_000_000));
    }

    #[test]
    fn parse_memory_limit_raw_bytes() {
        assert_eq!(parse_memory_limit("134217728"), Some(134_217_728));
        assert_eq!(parse_memory_limit("  134217728  "), Some(134_217_728));
    }

    #[test]
    fn parse_memory_limit_invalid_returns_none() {
        // Regression guard: the old implementation returned Some(0) for "128Mi"
        // because lowercase+trim_end_matches('m') left "128i" which parse::<f64>
        // rejected. The new implementation must never return Some(0) or Some of
        // a negative number from any input.
        assert_eq!(parse_memory_limit(""), None);
        assert_eq!(parse_memory_limit("   "), None);
        assert_eq!(parse_memory_limit("abc"), None);
        assert_eq!(parse_memory_limit("0"), None);
        assert_eq!(parse_memory_limit("0Mi"), None);
        assert_eq!(parse_memory_limit("-1Mi"), None);
    }

    #[test]
    fn parse_memory_limit_tolerates_whitespace_and_fractional() {
        assert_eq!(
            parse_memory_limit(" 1.5Gi "),
            Some((1.5 * (1024.0 * 1024.0 * 1024.0)) as i64)
        );
    }
}
