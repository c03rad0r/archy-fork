//! Render and lifecycle Quadlet `.container` units for companion UI
//! containers (archy-bitcoin-ui, archy-lnd-ui, archy-electrs-ui).
//!
//! Why Quadlet: companions used to run as fire-and-forget `tokio::spawn`
//! blocks from `install.rs`. If archipelago crashed mid-spawn or the
//! kernel reaped a parent cgroup, companions vanished from `podman ps`
//! entirely and only a manual `podman run` brought them back. Putting the
//! unit on disk and letting systemd own start/restart removes that whole
//! class of failure: the daemon is now systemd, archipelago is just the
//! provisioner.
//!
//! Design constraints kept this module small on purpose:
//!
//! - **Single responsibility**: render → write → enable → disable. We do
//!   NOT pull images here — the caller is expected to have the image
//!   present locally (companions either build from `/opt/archipelago/docker/`
//!   or are pre-pulled by `install_companion_image`). The quadlet unit
//!   declares `Pull=never` so a missing image surfaces immediately
//!   instead of silently retrying behind systemd's restart loop.
//! - **Atomic writes**: `tempfile + rename` so a partially-written unit
//!   is never visible to systemd. A daemon-reload during a rolling
//!   update can't see half a file.
//! - **Idempotent**: `write_if_changed` compares bytes before touching
//!   the file. No daemon-reload, no service-restart cascade if the
//!   rendered bytes match what's on disk.
//! - **systemctl --user only**: archipelago runs as uid=1000 with
//!   linger enabled. We never touch the system bus from here.
//!
//! See `docs/rust-orchestrator-migration.md` and the failure-mode log in
//! `feedback_container_lifecycle_failure_modes.md` for the incident
//! that motivated the move.

use anyhow::{anyhow, Context, Result};
use archipelago_container::AppManifest;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;

const QUADLET_START_TIMEOUT: Duration = Duration::from_secs(90);
const QUADLET_STOP_TIMEOUT: Duration = Duration::from_secs(45);

/// Default rootless quadlet directory. Resolved per-user at runtime via
/// `unit_dir()`. Tests pass an explicit dir.
pub const DEFAULT_REL_UNIT_DIR: &str = ".config/containers/systemd";

#[derive(Debug, Clone)]
pub struct BindMount {
    pub host: PathBuf,
    pub container: PathBuf,
    pub read_only: bool,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Bridge is reserved for Phase 5 per-app network isolation.
pub enum NetworkMode {
    #[default]
    Default,
    /// Host networking is only for companion/proxy containers that need to
    /// reach node-local daemons directly. It cannot be combined with
    /// PublishPort because Podman discards port mappings in host mode.
    Host,
    /// A user-defined podman network — quadlet creates the container
    /// attached to it. The network must already exist (orchestrator's
    /// `ensure_container_network` handles that on every reconcile tick).
    Bridge(String),
    /// Rootless slirp4netns networking. Podman rejects network aliases with
    /// this mode, so render only Network=slirp4netns.
    Slirp4netns,
    /// Rootless pasta networking. This is more reliable than slirp4netns for
    /// host port forwarding on long-running web apps.
    Pasta,
}

/// systemd Restart= policy for the generated `.service` unit.
///
/// Everything archipelago generates uses Always. Backends used to use
/// OnFailure, justified as "clean exits — e.g. operator-issued `systemctl
/// stop` — stay stopped". That justification was wrong on systemd's own
/// semantics: `Restart=` is never consulted for a unit stopped via
/// `systemctl stop` (systemd.service(5): "the service is not restarted if it
/// is stopped with systemctl stop or an equivalent operation"), and that is
/// exactly how archipelago stops these apps
/// (`prod_orchestrator` → `stop_service_with_timeout`). So OnFailure bought
/// none of the behaviour it claimed to.
///
/// What it cost was apps vanishing. Quadlet renders `podman run … --rm`, so
/// the container is deleted the moment it stops; OnFailure then declines to
/// restart after a CLEAN exit, and bitcoind exits 0 on SIGTERM. Any clean
/// stop therefore deleted the container AND left it deleted, so the app
/// disappeared from podman and from My Apps until a later archipelago
/// reconcile tick recreated it — logged as "previously-running app has no
/// container after boot — recreating" for bitcoin-knots and electrumx
/// repeatedly on 2026-08-07, and reported by the operator as "Bitcoin Knots
/// disappeared again". A crash always self-healed; only a clean exit
/// stranded it, which is why this hid for so long.
///
/// Always also restores the premise of the Quadlet migration: systemd owns
/// supervision, so an app comes back without needing archipelago alive to
/// notice. Safe against restart-looping because no manifest declares a
/// one-shot container and there is no manifest-level restart field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    Always,
    /// Retained as a deliberate opt-in for a unit that genuinely must stay
    /// down after a clean exit. Nothing selects it today — do not wire it to
    /// backends again without re-reading the note above.
    OnFailure,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::Always
    }
}

impl RestartPolicy {
    fn as_systemd(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OnFailure => "on-failure",
        }
    }
}

/// Container healthcheck wired through to Podman.
/// Systemd should consider the unit started once the container process is
/// running; health probes are app status, not boot ordering. Blocking
/// `systemctl start` on health made boot reconciliation hang when an image
/// lacked the probe helper binary, even though the service itself was live.
///
/// Ranges roughly mirror the manifest's HealthCheck struct: `cmd` is the
/// shell form (`/usr/bin/curl -fsS http://localhost:8332/health` etc.),
/// `interval`/`timeout` use systemd time format ("30s", "5m"), `retries`
/// is the consecutive-failures threshold before "unhealthy" trips.
#[derive(Debug, Clone)]
pub struct HealthSpec {
    pub cmd: String,
    pub interval: String,
    pub timeout: String,
    pub retries: u32,
}

/// One Quadlet `.container` unit. Field set is deliberately small —
/// add a new field only when a real manifest needs it.
#[derive(Debug, Clone, Default)]
pub struct QuadletUnit {
    pub name: String,
    pub description: String,
    pub image: String,
    pub network: NetworkMode,
    pub user: Option<String>,
    pub memory_mb: Option<u32>,
    pub cap_drop_all: bool,
    pub cap_add: Vec<String>,
    pub bind_mounts: Vec<BindMount>,
    pub extra_podman_args: Vec<String>,
    pub depends_on: Vec<String>,
    /// Phase 3.4: when present the rendered unit emits HealthCmd=,
    /// HealthInterval=, HealthTimeout=, and HealthRetries= for Podman's
    /// health state without blocking systemd's start job.
    pub health: Option<HealthSpec>,
    // Backend-manifest extensions (Phase 3.1). Companion units leave
    // these defaulted; the renderer skips empty/false directives so a
    // companion's rendered bytes are unchanged from before this PR.
    /// (host_port, container_port, protocol, bind). `bind` empty = 0.0.0.0.
    pub ports: Vec<(u16, u16, String, String)>,
    pub environment: Vec<String>,
    /// Secret-backed env: (env_key, podman secret name). Rendered as
    /// `Secret=<name>,type=env,target=<key>` so the VALUE never lands in
    /// this unit file on disk — only a reference to the podman secret
    /// store. The orchestrator registers the secrets before writing units.
    pub secret_env: Vec<(String, String)>,
    /// Container labels (`Label=k=v`). Carries the secret-env content hash
    /// for rotation-drift detection.
    pub labels: Vec<(String, String)>,
    pub devices: Vec<String>,
    pub add_hosts: Vec<(String, String)>,
    pub network_aliases: Vec<String>,
    pub entrypoint: Option<Vec<String>>,
    pub command: Vec<String>,
    pub read_only_root: bool,
    pub no_new_privileges: bool,
    pub cpu_quota: Option<u32>,
    pub restart_policy: RestartPolicy,
    pub stop_grace_secs: Option<u64>,
}

impl QuadletUnit {
    /// File name on disk: `<name>.container`. Quadlet translates this
    /// into a `<name>.service` unit at daemon-reload time.
    pub fn unit_filename(&self) -> String {
        format!("{}.container", self.name)
    }

    /// systemd service name created by Quadlet for this unit.
    pub fn service_name(&self) -> String {
        format!("{}.service", self.name)
    }

    /// Render the canonical Quadlet unit text. Pure function — no I/O.
    pub fn render(&self) -> String {
        let mut s = String::with_capacity(512);
        let _ = writeln!(s, "# Generated by archipelago. DO NOT EDIT.");
        let _ = writeln!(s, "# Edits are overwritten on the next reconcile.");
        let _ = writeln!(s);
        let _ = writeln!(s, "[Unit]");
        let _ = writeln!(s, "Description={}", self.description);
        let _ = writeln!(s, "After=network-online.target");
        let _ = writeln!(s, "Wants=network-online.target");
        for dep in &self.depends_on {
            let _ = writeln!(s, "Requires={dep}");
            let _ = writeln!(s, "After={dep}");
        }
        let _ = writeln!(s);
        let _ = writeln!(s, "[Container]");
        let _ = writeln!(s, "ContainerName={}", self.name);
        let _ = writeln!(s, "Image={}", self.image);
        let grace = self
            .stop_grace_secs
            .unwrap_or_else(|| archipelago_container::runtime::stop_grace_secs_for(&self.name));
        let _ = writeln!(s, "StopTimeout={grace}");
        // Pull=never: companions are pre-pulled or built. A missing image
        // must surface as a unit start failure, not a silent retry storm.
        let _ = writeln!(s, "Pull=never");
        match &self.network {
            NetworkMode::Default => {}
            NetworkMode::Host => {
                let _ = writeln!(s, "Network=host");
            }
            NetworkMode::Slirp4netns => {
                let _ = writeln!(s, "Network=slirp4netns");
            }
            NetworkMode::Pasta => {
                let _ = writeln!(s, "Network=pasta");
            }
            NetworkMode::Bridge(net) => {
                let _ = writeln!(s, "Network={net}");
                for alias in &self.network_aliases {
                    let _ = writeln!(s, "NetworkAlias={alias}");
                    let _ = writeln!(s, "PodmanArgs=--network-alias={alias}");
                }
            }
        }
        if let Some(user) = &self.user {
            let _ = writeln!(s, "User={user}");
        }
        if self.cap_drop_all {
            let _ = writeln!(s, "DropCapability=ALL");
        }
        for cap in &self.cap_add {
            let _ = writeln!(s, "AddCapability={cap}");
        }
        if let Some(mb) = self.memory_mb {
            let _ = writeln!(s, "PodmanArgs=--memory={mb}m");
        }
        for bm in &self.bind_mounts {
            let mode = if bm.read_only { ":ro,Z" } else { ":Z" };
            let _ = writeln!(
                s,
                "Volume={}:{}{}",
                bm.host.display(),
                bm.container.display(),
                mode
            );
        }
        // Host networking exposes the container's ports on the host directly.
        // Podman rejects PublishPort combined with Network=host ("published
        // ports cannot be used with host network") and the unit crash-loops
        // (exit 125). Skip publishing in host mode — matches the NetworkMode
        // doc note that Podman discards port mappings under host networking.
        if !matches!(self.network, NetworkMode::Host) {
            for (host, container, proto, bind) in &self.ports {
                let p = if proto.is_empty() {
                    "tcp"
                } else {
                    proto.as_str()
                };
                // Unbound publishes are pinned to 0.0.0.0: rootlessport's
                // wildcard bind claims [::] too but BLACK-HOLES inbound v6
                // (accepts, forwards nothing — "empty response" from the
                // mesh, confirmed 2026-07-26). Pinning v4 frees the port's
                // v6 side for the daemon's mesh relay (app_port_v6_relay_loop),
                // which forwards to the v4 loopback listener that works.
                // NOTE: this changes the rendered directive for unbound
                // ports on purpose — the one-time drift/recreate at upgrade
                // is the deploy vehicle for the fix.
                if bind.is_empty() {
                    let _ = writeln!(s, "PublishPort=0.0.0.0:{host}:{container}/{p}");
                } else {
                    let _ = writeln!(s, "PublishPort={bind}:{host}:{container}/{p}");
                }
            }
        }
        for env in &self.environment {
            // env entries already arrive shaped as "KEY=VALUE"; quadlet
            // accepts that form on a single Environment= line per pair.
            let _ = writeln!(s, "Environment={}", quote_environment(env));
        }
        for (key, secret_name) in &self.secret_env {
            let _ = writeln!(s, "Secret={secret_name},type=env,target={key}");
        }
        for (k, v) in &self.labels {
            let _ = writeln!(s, "Label={k}={v}");
        }
        for dev in &self.devices {
            let _ = writeln!(s, "AddDevice={dev}");
        }
        for (name, ip) in &self.add_hosts {
            let _ = writeln!(s, "AddHost={name}:{ip}");
        }
        if self.read_only_root {
            let _ = writeln!(s, "ReadOnly=true");
        }
        if self.no_new_privileges {
            let _ = writeln!(s, "NoNewPrivileges=true");
        }
        if let Some(cpus) = self.cpu_quota {
            let _ = writeln!(s, "PodmanArgs=--cpus={cpus}");
        }
        if let Some(h) = &self.health {
            let _ = writeln!(s, "HealthCmd={}", h.cmd.replace('%', "%%"));
            let _ = writeln!(s, "HealthInterval={}", h.interval);
            let _ = writeln!(s, "HealthTimeout={}", h.timeout);
            let _ = writeln!(s, "HealthRetries={}", h.retries);
        }
        if let Some((first, rest)) = self.entrypoint.as_deref().and_then(<[String]>::split_first) {
            // Quadlet's Exec= sets only the command (the args passed to the
            // image's ENTRYPOINT) — it does NOT replace the entrypoint. So a
            // manifest entrypoint like `sh -lc` must be emitted as a real
            // Entrypoint= override; otherwise it gets appended to whatever
            // ENTRYPOINT the image baked in (e.g. the versioned bitcoind
            // images use `ENTRYPOINT ["bitcoind"]`, which turned the wrapper
            // into `bitcoind sh -lc ...` and crash-looped). Emitting
            // Entrypoint= makes the unit independent of the image's entrypoint.
            let _ = writeln!(s, "Entrypoint={}", first.replace('%', "%%"));
            let mut parts: Vec<String> = rest.to_vec();
            parts.extend(self.command.iter().cloned());
            if !parts.is_empty() {
                let _ = writeln!(s, "Exec={}", shell_join(&parts));
            }
        } else if !self.command.is_empty() {
            let _ = writeln!(s, "Exec={}", shell_join(&self.command));
        }
        for arg in &self.extra_podman_args {
            let _ = writeln!(s, "PodmanArgs={arg}");
        }
        let _ = writeln!(s);
        let _ = writeln!(s, "[Service]");
        // Dependency-gated apps may legitimately keep their container entrypoint
        // in a wait loop before the actual daemon binds ports. Fedimint waits
        // for Bitcoin IBD to finish before execing fedimintd; systemd's default
        // start timeout otherwise kills the generated podman run job and leaves
        // the unit stuck in deactivating. Health/status remains app-level state,
        // not a systemd start gate.
        let _ = writeln!(s, "TimeoutStartSec=0");
        let _ = writeln!(s, "TimeoutStopSec={}", grace.saturating_add(15));
        // Stop explicitly before Quadlet's generated `podman rm -f`. The
        // existing container may still carry Podman's old 10-second default;
        // StopTimeout alone only protects containers created after migration.
        let _ = writeln!(s, "ExecStop=");
        let _ = writeln!(
            s,
            "ExecStop=/usr/bin/podman stop --ignore --time={grace} --cidfile=%t/%N.cid"
        );
        // Restart policy + 10s backoff. RestartSec keeps a crash-loop
        // from saturating the journal. Companions: Always. Backends:
        // OnFailure (clean stops stay stopped).
        let _ = writeln!(s, "Restart={}", self.restart_policy.as_systemd());
        let _ = writeln!(s, "RestartSec=10");
        let _ = writeln!(s);
        let _ = writeln!(s, "[Install]");
        let _ = writeln!(s, "WantedBy=default.target");
        s
    }
}

/// Render a manifest's argv-style list as a single Exec= line. We do
/// the minimum quoting needed so quadlet's parser sees one element per
/// item: anything containing whitespace, quotes, or shell metacharacters
/// gets wrapped in double quotes with embedded `"` and `\` escaped.
///
/// `%` is escaped to `%%`: quadlet copies Exec= content into the generated
/// service's ExecStart, where systemd expands `%` specifiers at load time —
/// a bare `%s` in a manifest script silently became `/bin/bash` (the user's
/// shell) and corrupted bitcoind's generated rpc.conf.
fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|p| {
            let p = p.replace(['\r', '\n'], " ").replace('%', "%%");
            if p.is_empty()
                || p.chars()
                    .any(|c| c.is_whitespace() || "'\"\\$`".contains(c))
            {
                let escaped = p
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('$', "$$");
                format!("\"{escaped}\"")
            } else {
                p
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_environment(env: &str) -> String {
    // `%` → `%%` for the same systemd-specifier reason as shell_join.
    let env = env.replace(['\r', '\n'], " ").replace('%', "%%");
    if env.is_empty()
        || env
            .chars()
            .any(|c| c.is_whitespace() || "'\"\\$`".contains(c))
    {
        let escaped = env
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "$$");
        format!("\"{escaped}\"")
    } else {
        env
    }
}

impl QuadletUnit {
    /// Build a backend-flavour QuadletUnit from a parsed AppManifest.
    /// Wired through `prod_orchestrator::install_via_quadlet`, gated by
    /// `Config::use_quadlet_backends`.
    ///
    /// `name` is the on-disk container name (typically the manifest's
    /// `app.id`, but the orchestrator may rename — see
    /// `compute_container_name`). The returned unit is NOT yet written;
    /// the caller is expected to merge in any environment overrides
    /// (resolve_dynamic_env, secret_env) before calling write_if_changed.
    pub fn from_manifest(manifest: &AppManifest, name: &str) -> Self {
        let app = &manifest.app;

        let network = match app.security.network_policy.as_str() {
            "host" => NetworkMode::Host,
            // Bridge name comes from the manifest's container.network if
            // set; otherwise the orchestrator manages a default network
            // separately and we fall back to host. Quadlet won't refuse
            // either form.
            other if !other.is_empty() && other != "isolated" => NetworkMode::Bridge(other.into()),
            _ => match app.container.network.as_deref() {
                Some("slirp4netns") => NetworkMode::Slirp4netns,
                Some("pasta") => NetworkMode::Pasta,
                Some(n) if !n.is_empty() && n != "host" => NetworkMode::Bridge(n.into()),
                _ => NetworkMode::Default,
            },
        };

        let bind_mounts = app
            .volumes
            .iter()
            .filter(|v| v.volume_type != "tmpfs" && !v.source.is_empty())
            .map(|v| BindMount {
                host: PathBuf::from(&v.source),
                container: PathBuf::from(&v.target),
                read_only: v.options.iter().any(|o| o == "ro"),
            })
            .collect::<Vec<_>>();

        let memory_mb = app.resources.memory_limit.as_ref().and_then(|s| {
            // Manifests use forms like "1g", "512m", "1024". Convert to
            // MiB. Anything we can't parse gets dropped (renderer skips
            // None) — better to lose the limit than to mis-cap.
            parse_memory_mib(s)
        });

        Self {
            name: name.to_string(),
            description: format!("Archipelago app: {}", app.id),
            image: app.container.image_ref().unwrap_or_default(),
            network,
            user: None,
            memory_mb,
            cap_drop_all: true,
            cap_add: app.security.capabilities.clone(),
            bind_mounts,
            extra_podman_args: vec![],
            depends_on: vec![],
            health: app.health_check.as_ref().and_then(translate_health_check),
            ports: app
                .ports
                .iter()
                .filter(|p| {
                    let ok = archipelago_container::manifest::host_can_bind_publish_ip(&p.bind);
                    if !ok {
                        tracing::warn!(
                            app = %app.id,
                            bind = %p.bind,
                            host_port = p.host,
                            "dropping publish: bind address not assignable on this host \
                             (rootlessport would crash-loop the unit)"
                        );
                    }
                    ok
                })
                .map(|p| (p.host, p.container, p.protocol.clone(), p.bind.clone()))
                .collect(),
            environment: app.environment.clone(),
            secret_env: app
                .container
                .secret_env_refs
                .iter()
                .map(|r| (r.env_key.clone(), r.secret_name.clone()))
                .collect(),
            labels: app
                .container
                .secret_env_hash
                .iter()
                .map(|h| {
                    (
                        archipelago_container::manifest::SECRET_ENV_HASH_LABEL.to_string(),
                        h.clone(),
                    )
                })
                .collect(),
            devices: app.devices.clone(),
            add_hosts: vec![("host.archipelago".into(), "10.89.0.1".into())],
            // Container always answers to its own name; manifest extras add the
            // short hostnames peers bake in (e.g. indeedhub api/minio/relay).
            // Only emitted for Bridge networks (slirp/pasta reject aliases).
            network_aliases: {
                let mut a = vec![name.to_string()];
                for extra in &app.container.network_aliases {
                    if !a.iter().any(|x| x == extra) {
                        a.push(extra.clone());
                    }
                }
                a
            },
            entrypoint: app.container.entrypoint.clone(),
            command: app.container.custom_args.clone(),
            read_only_root: app.security.readonly_root,
            no_new_privileges: app.security.no_new_privileges,
            cpu_quota: app.resources.cpu_limit,
            // Always, not OnFailure: with quadlet's `--rm`, OnFailure left a
            // cleanly-exited app deleted and unrestarted. See RestartPolicy.
            restart_policy: RestartPolicy::Always,
            stop_grace_secs: Some(super::prod_orchestrator::resolve_stop_grace_secs(
                manifest, name,
            )),
        }
    }
}

/// Translate the manifest's HealthCheck shape into a HealthSpec the
/// renderer understands. Returns None when the manifest's health spec
/// is malformed or unsupported rather than emitting a broken HealthCmd.
///
/// Supported shapes:
///   - type: tcp,  endpoint: "host:port"        → skipped for Quadlet units
///   - type: http, endpoint: "host:port" or "http(s)://host:port", path → wget/curl
///   - type: cmd,  endpoint: "<shell command>"  → `<shell command>` verbatim
///
/// For type=http we accept the endpoint with or without scheme; manifests
/// in the wild use both forms (`localhost:8175` and
/// `http://localhost:8175/`). Earlier we blindly prepended `http://` even
/// when one was already there, producing `http://http://...` HealthCmds
/// that pasted on .228 2026-05-02 and failed every probe.
fn translate_health_check(hc: &archipelago_container::HealthCheck) -> Option<HealthSpec> {
    let cmd = match hc.check_type.as_str() {
        // A generic TCP probe inside arbitrary app images is not reliable:
        // some images lack nc, some lack bash /dev/tcp, and failures leave
        // Podman/systemd health in a false-negative state. Keep TCP readiness
        // checks in the host-side lifecycle/status layer instead.
        "tcp" => return None,
        "http" => {
            let endpoint = hc.endpoint.as_deref()?.trim();
            // Accept either bare host:port or a full URL. If endpoint
            // already includes a scheme we use it as-is; otherwise we
            // prepend http://. This keeps existing http://foo manifests
            // working and stops the http://http:// double-prefix bug.
            let url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                endpoint.to_string()
            } else {
                format!("http://{endpoint}")
            };
            // If the endpoint already carried a path component, honour it
            // and ignore hc.path (manifests that bake the path into the
            // endpoint don't expect to merge a separate path field).
            // Otherwise append hc.path (default "/").
            let already_has_path = url
                .splitn(4, '/')
                .nth(3)
                .map(|p| !p.is_empty())
                .unwrap_or(false);
            let final_url = if already_has_path {
                url
            } else {
                let path = hc.path.as_deref().unwrap_or("/");
                format!("{url}{path}")
            };
            let helper_timeout = health_timeout_seconds(&hc.timeout);
            let (tcp_host, tcp_port) = health_url_host_port(&final_url);
            // Images vary wildly: SearXNG ships wget but no curl, while some
            // images (btcpay's dotnet base) ship neither but do have bash —
            // fall through to a bash /dev/tcp connect probe there so the
            // container can't report healthy while its port is dead
            // (btcpay-server, 2026-07-10). Podman's own health timeout bounds
            // the connect attempt. Only images with none of the three skip
            // Podman health entirely; host-side lifecycle probes still verify
            // reachability for those.
            format!(
                "if command -v wget >/dev/null 2>&1; then wget -q -T {1} -O /dev/null {0}; elif command -v curl >/dev/null 2>&1; then curl -fsS -m {1} {0}; elif command -v bash >/dev/null 2>&1; then bash -c 'exec 3<>/dev/tcp/{2}/{3}'; else exit 0; fi",
                final_url, helper_timeout, tcp_host, tcp_port
            )
        }
        "cmd" => hc.endpoint.as_deref()?.to_string(),
        _ => return None,
    };
    Some(HealthSpec {
        cmd,
        interval: hc.interval.clone(),
        timeout: hc.timeout.clone(),
        retries: hc.retries,
    })
}

/// Host and port of an http(s) health URL, for the bash /dev/tcp fallback
/// probe. Defaults to port 80/443 by scheme when the authority carries none.
fn health_url_host_port(url: &str) -> (String, u16) {
    let (default_port, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (443, rest)
    } else {
        (80, url.strip_prefix("http://").unwrap_or(url))
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    match authority.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(port) => (host.to_string(), port),
            Err(_) => (authority.to_string(), default_port),
        },
        None => (authority.to_string(), default_port),
    }
}

fn health_timeout_seconds(raw: &str) -> u64 {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 5;
    }

    let (number, multiplier) = match trimmed.chars().last() {
        Some('s') | Some('S') => (&trimmed[..trimmed.len() - 1], 1),
        Some('m') | Some('M') => (&trimmed[..trimmed.len() - 1], 60),
        Some('h') | Some('H') => (&trimmed[..trimmed.len() - 1], 3600),
        Some(c) if c.is_ascii_digit() => (trimmed, 1),
        _ => return 5,
    };

    number
        .trim()
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(multiplier))
        .filter(|n| *n > 0)
        .unwrap_or(5)
}

/// Parse the manifest's memory_limit string into MiB. Recognises the
/// forms our manifests actually use: "<n>", "<n>m"/"<n>M", "<n>g"/"<n>G".
/// Returns None for anything else; the caller treats None as unlimited.
fn parse_memory_mib(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (num_part, mul) = match trimmed.chars().last()? {
        'g' | 'G' => (&trimmed[..trimmed.len() - 1], 1024u32),
        'm' | 'M' => (&trimmed[..trimmed.len() - 1], 1u32),
        'k' | 'K' => return None, // sub-MiB precision: drop, not worth it
        c if c.is_ascii_digit() => (trimmed, 1u32), // bare number, treat as MiB
        _ => return None,
    };
    num_part.trim().parse::<u32>().ok()?.checked_mul(mul)
}

/// Does a quadlet `.container` unit exist for this container name?
/// Errors count as "unknown" and return false — callers use this to decide
/// whether systemd owns the container, and claiming ownership on an
/// unreadable answer would route lifecycle ops around a live unit.
pub async fn unit_exists(name: &str) -> bool {
    let Ok(dir) = unit_dir().await else {
        return false;
    };
    tokio::fs::try_exists(dir.join(format!("{name}.container")))
        .await
        .unwrap_or(false)
}

/// Resolve the per-user quadlet dir under $HOME. Created if missing.
pub async fn unit_dir() -> Result<PathBuf> {
    #[cfg(test)]
    {
        static TEST_UNITS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        return Ok(TEST_UNITS
            .get_or_init(|| tempfile::tempdir().unwrap().keep())
            .clone());
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME not set; cannot locate quadlet unit dir"))?;
    let dir = home.join(DEFAULT_REL_UNIT_DIR);
    fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create_dir_all {}", dir.display()))?;
    Ok(dir)
}

/// Atomically write `unit` into `dir/<name>.container` if the bytes
/// differ from what's already there. Returns true if the file changed.
pub async fn write_if_changed(unit: &QuadletUnit, dir: &Path) -> Result<bool> {
    let path = dir.join(unit.unit_filename());
    let new_bytes = unit.render();

    if let Ok(old) = fs::read_to_string(&path).await {
        if old == new_bytes {
            return Ok(false);
        }
    }

    fs::create_dir_all(dir)
        .await
        .with_context(|| format!("create_dir_all {}", dir.display()))?;
    let tmp = path.with_extension("container.tmp");
    fs::write(&tmp, new_bytes.as_bytes())
        .await
        .with_context(|| format!("write tmp {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .await
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(true)
}

/// Reload the user systemd manager. Required after any quadlet write
/// or removal so systemd picks up the generated `.service` translation.
pub async fn daemon_reload_user() -> Result<()> {
    // Bounded: a wedged user manager (e.g. a unit stuck "deactivating" while
    // podman hangs) could otherwise block daemon-reload indefinitely and freeze
    // any caller — notably uninstall teardown.
    let status = systemctl_user_status(&["daemon-reload"], Duration::from_secs(30))
        .await
        .context("systemctl --user daemon-reload")?;
    if !status.success() {
        return Err(anyhow!("systemctl --user daemon-reload exited {status}"));
    }
    Ok(())
}

/// Enable + start a quadlet-generated service. `enable --now` makes it
/// survive reboots and starts it immediately.
pub async fn enable_now(service: &str) -> Result<()> {
    // Quadlet-generated units cannot be `enable`d directly because the
    // .service file lives under /run, not /etc — `enable` would refuse
    // ("transient or generated"). The unit's `[Install] WantedBy` is
    // honoured at daemon-reload, so we just start it.
    let status = systemctl_user_status(&["start", service], QUADLET_START_TIMEOUT)
        .await
        .with_context(|| format!("systemctl --user start {service}"))?;
    if !status.success() {
        if wait_not_deactivating(service, Duration::from_secs(30)).await {
            let retry = systemctl_user_status(&["start", service], QUADLET_START_TIMEOUT)
                .await
                .with_context(|| format!("retry systemctl --user start {service}"))?;
            if retry.success() {
                return Ok(());
            }
            return Err(anyhow!(
                "systemctl --user start {service} exited {status}; retry exited {retry}"
            ));
        }
        return Err(anyhow!("systemctl --user start {service} exited {status}"));
    }
    Ok(())
}

/// Restart a generated Quadlet service after rewriting a known-bad unit.
pub async fn restart_service(service: &str) -> Result<()> {
    // `systemctl restart` hides the stop phase. On rootless Podman nodes a
    // generated unit can sit in deactivating while `podman rm -f` hangs, which
    // makes RPC/UI state look frozen. Split restart into bounded stop + start
    // so stop timeouts can be recovered with an app-scoped kill/reset.
    if let Err(err) = stop_service(service).await {
        tracing::warn!(
            service = %service,
            error = %err,
            "quadlet stop failed during restart; waiting for unit to settle before start"
        );
    }
    if !wait_not_deactivating(service, Duration::from_secs(120)).await {
        return Err(anyhow!(
            "systemctl --user restart {service} could not leave deactivating state"
        ));
    }
    enable_now(service).await
}

/// Stop a generated Quadlet service without removing its unit file.
pub async fn stop_service(service: &str) -> Result<()> {
    stop_service_with_timeout(service, QUADLET_STOP_TIMEOUT).await
}

/// Stop a user service, waiting up to `timeout` for a graceful stop before
/// force-killing the app-scoped unit. Slow-to-SIGTERM apps (bitcoin-core ~600s,
/// lnd ~330s) must not be SIGKILLed at the default 45s — that risks data
/// corruption — so the orchestrator passes the per-app grace here. Never waits
/// less than `QUADLET_STOP_TIMEOUT`.
pub async fn stop_service_with_timeout(service: &str, timeout: Duration) -> Result<()> {
    let name = service.strip_suffix(".service").unwrap_or(service);
    let body = fs::read_to_string(unit_dir().await?.join(format!("{name}.container")))
        .await
        .unwrap_or_default();
    let timeout = timeout.max(stop_wait_timeout(name, &body));
    match systemctl_user_status(&["stop", service], timeout).await {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(anyhow!("systemctl --user stop {service} exited {status}")),
        Err(err) => {
            tracing::warn!(
                service = %service,
                error = %err,
                "quadlet stop timed out/failed; killing app-scoped unit"
            );
            kill_and_reset_service(service).await?;
            if !wait_not_deactivating(service, Duration::from_secs(60)).await {
                return Err(anyhow!(
                    "systemctl --user stop {service} remained deactivating after app-scoped kill"
                ));
            }
            Ok(())
        }
    }
}

/// The command waiter must outlive both the container grace and systemd's
/// stop deadline. Restart/repair callers must not kill Bitcoin at 45 seconds.
fn stop_wait_timeout(name: &str, unit_body: &str) -> Duration {
    Duration::from_secs(stop_grace_from_unit(name, unit_body).saturating_add(30))
        .max(QUADLET_STOP_TIMEOUT)
}

fn stop_grace_from_unit(name: &str, unit_body: &str) -> u64 {
    directive_values(unit_body, "StopTimeout=")
        .last()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| archipelago_container::runtime::stop_grace_secs_for(name))
}

async fn systemctl_user_status(
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::ExitStatus> {
    #[cfg(test)]
    {
        use std::os::unix::process::ExitStatusExt;
        return Ok(std::process::ExitStatus::from_raw(0));
    }
    let mut cmd = Command::new("systemctl");
    cmd.arg("--user").args(args);
    cmd.kill_on_drop(true);
    tokio::time::timeout(timeout, cmd.status())
        .await
        .with_context(|| {
            format!(
                "systemctl --user {} timed out after {}s",
                args.join(" "),
                timeout.as_secs()
            )
        })?
        .with_context(|| format!("spawn systemctl --user {}", args.join(" ")))
}

async fn kill_and_reset_service(service: &str) -> Result<()> {
    let _ = systemctl_user_status(
        &["kill", "--kill-whom=all", "-s", "SIGKILL", service],
        Duration::from_secs(15),
    )
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = systemctl_user_status(&["reset-failed", service], Duration::from_secs(15)).await;
    Ok(())
}

async fn wait_not_deactivating(service: &str, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let Ok(status) =
            systemctl_user_output(&["is-active", service], Duration::from_secs(5)).await
        else {
            return true;
        };
        let state = String::from_utf8_lossy(&status.stdout).trim().to_string();
        if state != "deactivating" && state != "activating" {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn systemctl_user_output(args: &[&str], timeout: Duration) -> Result<std::process::Output> {
    #[cfg(test)]
    {
        anyhow::bail!("Unit tests have no real user service manager");
    }
    let mut cmd = Command::new("systemctl");
    cmd.arg("--user").args(args);
    cmd.kill_on_drop(true);
    tokio::time::timeout(timeout, cmd.output())
        .await
        .with_context(|| {
            format!(
                "systemctl --user {} timed out after {}s",
                args.join(" "),
                timeout.as_secs()
            )
        })?
        .with_context(|| format!("spawn systemctl --user {}", args.join(" ")))
}

pub fn contains_stale_health_gate(unit_body: &str) -> bool {
    unit_body.contains("Notify=healthy")
        || unit_body.contains("TimeoutStartSec=600")
        || unit_body.contains("HealthCmd=nc -z")
}

pub fn health_cmd_changed(old_body: &str, new_body: &str) -> bool {
    directive_values(old_body, "HealthCmd=") != directive_values(new_body, "HealthCmd=")
        || directive_values(old_body, "HealthInterval=")
            != directive_values(new_body, "HealthInterval=")
        || directive_values(old_body, "HealthTimeout=")
            != directive_values(new_body, "HealthTimeout=")
        || directive_values(old_body, "HealthRetries=")
            != directive_values(new_body, "HealthRetries=")
}

/// A unit rewrite and a successful systemd restart are separate operations.
/// Keep the restart obligation across errors or a management-daemon restart.
pub struct RestartObligation {
    marker: PathBuf,
    pending: bool,
}

impl RestartObligation {
    pub async fn prepare(unit_path: &Path, newly_required: bool) -> Result<Self> {
        let marker = unit_path.with_extension("restart-pending");
        if newly_required {
            // Contents contain no manifest environment or credentials. sync_all
            // makes the obligation durable before the subsequent unit rename.
            let file = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&marker)
                .await
                .context("record pending Quadlet restart")?;
            file.sync_all().await?;
            if let Some(parent) = marker.parent() {
                tokio::fs::File::open(parent).await?.sync_all().await?;
            }
        }
        let pending = tokio::fs::try_exists(&marker).await?;
        Ok(Self { marker, pending })
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Call only after systemd accepted the replacement service successfully.
    pub async fn complete(self) -> Result<()> {
        if self.pending {
            tokio::fs::remove_file(&self.marker)
                .await
                .context("clear completed Quadlet restart")?;
            if let Some(parent) = self.marker.parent() {
                tokio::fs::File::open(parent).await?.sync_all().await?;
            }
        }
        Ok(())
    }
}

pub fn publish_ports_changed(old_body: &str, new_body: &str) -> bool {
    let old_ports = directive_values(old_body, "PublishPort=");
    let new_ports = directive_values(new_body, "PublishPort=");
    old_ports != new_ports
}

pub fn security_changed(old_body: &str, new_body: &str) -> bool {
    [
        "AddCapability=",
        "DropCapability=",
        "NoNewPrivileges=",
        "ReadOnly=",
        "User=",
    ]
    .iter()
    .any(|directive| {
        let mut old = directive_values(old_body, directive);
        let mut new = directive_values(new_body, directive);
        old.sort();
        new.sort();
        old != new
    })
}

pub fn network_aliases_changed(old_body: &str, new_body: &str) -> bool {
    let old_network = directive_values(old_body, "Network=");
    let new_network = directive_values(new_body, "Network=");
    let old_aliases = directive_values(old_body, "NetworkAlias=");
    let new_aliases = directive_values(new_body, "NetworkAlias=");
    old_network != new_network || old_aliases != new_aliases
}

pub fn exec_changed(old_body: &str, new_body: &str) -> bool {
    // Entrypoint= and Exec= together define what the container runs, so a drift
    // in either must recreate the container (e.g. when this renderer first
    // splits a folded `Exec=sh -lc ...` into `Entrypoint=sh` + `Exec=-lc ...`).
    directive_values(old_body, "Exec=") != directive_values(new_body, "Exec=")
        || directive_values(old_body, "Entrypoint=") != directive_values(new_body, "Entrypoint=")
}

fn directive_values(unit_body: &str, prefix: &str) -> Vec<String> {
    unit_body
        .lines()
        .filter_map(|line| line.trim().strip_prefix(prefix))
        .map(str::to_string)
        .collect()
}

/// Stop + remove a quadlet unit and its on-disk file. Best-effort:
/// errors stop only the destructive write at the failing step so a
/// partial removal doesn't leave a quadlet file pointing at a service
/// that systemd no longer knows about.
pub async fn disable_remove(unit_name: &str, dir: &Path) -> Result<()> {
    let svc = format!("{unit_name}.service");
    let path = dir.join(format!("{unit_name}.container"));
    let body = fs::read_to_string(&path).await.unwrap_or_default();
    let timeout = stop_wait_timeout(unit_name, &body);
    let grace = stop_grace_from_unit(unit_name, &body).to_string();
    // Stop first; ignore failure (unit may already be down). BOUNDED — on
    // rootless podman a generated unit can wedge in "deactivating" while
    // `podman rm -f` hangs underneath it, and an unbounded `systemctl stop`
    // would block the entire uninstall forever: the progress bar freezes and
    // the package entry is stranded in `Removing` (a ghost in My Apps that also
    // blocks reinstall). If the graceful stop times out, escalate to
    // SIGKILL + reset-failed so teardown always proceeds.
    if systemctl_user_status(&["stop", &svc], timeout)
        .await
        .is_err()
    {
        let _ = kill_and_reset_service(&svc).await;
    }
    if fs::try_exists(&path).await.unwrap_or(false) {
        match fs::remove_file(&path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("remove {}", path.display())),
        }
    }
    daemon_reload_user().await.ok();
    // Defensive: kill the actual container too, in case quadlet left it.
    // Bounded so a hung podman store can't re-introduce the stall this function
    // exists to avoid.
    let _ = tokio::time::timeout(
        timeout,
        Command::new("podman")
            .args(["rm", "-f", "--ignore", "--time", &grace, unit_name])
            .status(),
    )
    .await;
    Ok(())
}

/// Is the quadlet-generated service currently active?
pub async fn is_active(service: &str) -> bool {
    if cfg!(test) {
        return false;
    }
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", service])
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn shutdown_grace_covers_container_systemd_and_caller() {
        for (name, grace) in [
            ("bitcoin-core", 600),
            ("bitcoin-knots", 600),
            ("lnd", 330),
            ("electrumx", 300),
            ("other", 30),
        ] {
            let unit = QuadletUnit {
                name: name.into(),
                ..Default::default()
            };
            let body = unit.render();
            assert!(body.contains(&format!("StopTimeout={grace}\n")));
            assert!(body.contains(&format!("TimeoutStopSec={}\n", grace + 15)));
            assert!(body.contains(&format!("podman stop --ignore --time={grace} --cidfile=")));
            assert_eq!(
                stop_wait_timeout(name, &body),
                Duration::from_secs(grace + 30)
            );
            // Legacy units have no StopTimeout directive yet.
            assert_eq!(stop_wait_timeout(name, ""), Duration::from_secs(grace + 30));
        }
    }

    #[test]
    fn custom_stop_grace_survives_render_and_restart_budget() {
        let manifest: AppManifest = serde_yaml::from_str(
            r#"
app:
  id: custom-db
  name: Custom database
  version: 1.0.0
  stop_grace_secs: 900
  container:
    image: example/db:1
"#,
        )
        .unwrap();
        let unit = QuadletUnit::from_manifest(&manifest, "custom-db");
        assert_eq!(unit.stop_grace_secs, Some(900));
        assert_eq!(
            stop_wait_timeout("custom-db", &unit.render()),
            Duration::from_secs(930)
        );
        assert_eq!(
            stop_wait_timeout("lnd", "StopTimeout=invalid"),
            Duration::from_secs(360)
        );
    }

    #[test]
    fn stop_grace_migration_does_not_request_an_execution_restart() {
        let unit = sample_unit();
        let new = unit.render();
        let old = new
            .lines()
            .filter(|line| {
                !line.starts_with("StopTimeout=")
                    && !line.starts_with("TimeoutStopSec=")
                    && !line.starts_with("ExecStop=")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!exec_changed(&old, &new));
        assert!(!publish_ports_changed(&old, &new));
        assert!(!network_aliases_changed(&old, &new));
        assert!(!health_cmd_changed(&old, &new));
    }

    #[test]
    fn actual_quadlet_generator_stops_before_forced_removal() {
        let generator = Path::new("/usr/lib/systemd/system-generators/podman-system-generator");
        if !generator.exists() {
            eprintln!(
                "Quadlet generator unavailable; run this regression on the Linux release host"
            );
            return;
        }
        let dir = tempdir().unwrap();
        let unit = QuadletUnit {
            name: "grace-test".into(),
            image: "localhost/test:latest".into(),
            stop_grace_secs: Some(600),
            ..Default::default()
        };
        std::fs::write(dir.path().join("grace-test.container"), unit.render()).unwrap();
        let output = std::process::Command::new(generator)
            .args(["--user", "--dryrun"])
            .env("QUADLET_UNIT_DIRS", dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let generated = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        let stop = generated
            .find("ExecStop=/usr/bin/podman stop --ignore --time=600")
            .unwrap();
        let remove = generated.find("ExecStop=/usr/bin/podman rm ").unwrap();
        assert!(
            stop < remove,
            "Legacy container must stop gracefully before removal"
        );
        assert!(generated.contains("--stop-timeout 600"));
        assert!(generated.contains("TimeoutStopSec=615"));
    }

    #[test]
    fn render_emits_secret_env_by_reference_never_value() {
        let u = QuadletUnit {
            name: "t".into(),
            description: "t".into(),
            image: "img".into(),
            secret_env: vec![("DB_PASS".into(), "archy-env-app-db_pass".into())],
            labels: vec![("io.archipelago.secret-env-hash".into(), "abc123".into())],
            ..QuadletUnit::default()
        };
        let s = u.render();
        assert!(s.contains("Secret=archy-env-app-db_pass,type=env,target=DB_PASS"));
        assert!(s.contains("Label=io.archipelago.secret-env-hash=abc123"));
        // the secret VALUE never had a path into this unit — but guard the
        // env channel anyway: no Environment= line may mention the key
        assert!(!s.contains("Environment=DB_PASS"));
    }

    fn sample_unit() -> QuadletUnit {
        QuadletUnit {
            name: "archy-bitcoin-ui".into(),
            description: "Bitcoin RPC UI proxy".into(),
            image: "source.archipelago-foundation.org/lfg2025/bitcoin-ui:1.7.84-alpha".into(),
            network: NetworkMode::Host,
            user: Some("0:0".into()),
            memory_mb: Some(128),
            cap_drop_all: true,
            cap_add: vec![
                "CHOWN".into(),
                "DAC_OVERRIDE".into(),
                "NET_BIND_SERVICE".into(),
                "SETUID".into(),
                "SETGID".into(),
            ],
            bind_mounts: vec![BindMount {
                host: PathBuf::from("/var/lib/archipelago/bitcoin-ui/nginx.conf"),
                container: PathBuf::from("/etc/nginx/conf.d/default.conf"),
                read_only: true,
            }],
            extra_podman_args: vec![],
            depends_on: vec![],
            ..QuadletUnit::default()
        }
    }

    #[test]
    fn render_contains_required_directives() {
        let s = sample_unit().render();
        assert!(s.contains("[Container]"));
        assert!(s.contains("ContainerName=archy-bitcoin-ui"));
        assert!(
            s.contains("Image=source.archipelago-foundation.org/lfg2025/bitcoin-ui:1.7.84-alpha")
        );
        assert!(s.contains("Pull=never"));
        assert!(s.contains("Network=host"));
        assert!(s.contains("DropCapability=ALL"));
        assert!(s.contains("AddCapability=CHOWN"));
        assert!(s.contains("AddCapability=NET_BIND_SERVICE"));
        assert!(s.contains("PodmanArgs=--memory=128m"));
        assert!(s.contains(
            "Volume=/var/lib/archipelago/bitcoin-ui/nginx.conf:/etc/nginx/conf.d/default.conf:ro,Z"
        ));
        assert!(s.contains("[Service]"));
        assert!(s.contains("Restart=always"));
        assert!(s.contains("WantedBy=default.target"));
    }

    #[test]
    fn render_bridge_network_emits_network_name() {
        let mut u = sample_unit();
        u.network = NetworkMode::Bridge("archy-bitcoin-ui-net".into());
        let s = u.render();
        assert!(s.contains("Network=archy-bitcoin-ui-net"));
        assert!(!s.contains("Network=host"));
    }

    #[test]
    fn render_host_network_omits_publish_ports() {
        // Podman rejects PublishPort with Network=host (crash-loop exit 125).
        let mut u = sample_unit();
        u.network = NetworkMode::Host;
        u.ports = vec![(3000, 3000, "tcp".into(), String::new())];
        let s = u.render();
        assert!(s.contains("Network=host"));
        assert!(!s.contains("PublishPort"));
    }

    #[test]
    fn render_non_host_network_emits_publish_ports() {
        let mut u = sample_unit();
        u.network = NetworkMode::Bridge("archy-net".into());
        u.ports = vec![(3000, 3000, "tcp".into(), String::new())];
        let s = u.render();
        assert!(s.contains("PublishPort=0.0.0.0:3000:3000/tcp"));
    }

    #[test]
    fn unit_filename_and_service_name_are_consistent() {
        let u = sample_unit();
        assert_eq!(u.unit_filename(), "archy-bitcoin-ui.container");
        assert_eq!(u.service_name(), "archy-bitcoin-ui.service");
    }

    #[tokio::test]
    async fn write_if_changed_writes_first_time_then_noops() {
        let dir = tempdir().unwrap();
        let u = sample_unit();
        let changed = write_if_changed(&u, dir.path()).await.unwrap();
        assert!(changed, "first write must report changed");
        let on_disk = tokio::fs::read_to_string(dir.path().join(u.unit_filename()))
            .await
            .unwrap();
        assert!(on_disk.starts_with("# Generated by archipelago"));

        let changed2 = write_if_changed(&u, dir.path()).await.unwrap();
        assert!(!changed2, "second write with identical bytes must no-op");
    }

    #[tokio::test]
    async fn write_if_changed_rewrites_when_field_changes() {
        let dir = tempdir().unwrap();
        let mut u = sample_unit();
        write_if_changed(&u, dir.path()).await.unwrap();

        u.memory_mb = Some(256);
        let changed = write_if_changed(&u, dir.path()).await.unwrap();
        assert!(changed, "field change must trigger rewrite");
        let on_disk = tokio::fs::read_to_string(dir.path().join(u.unit_filename()))
            .await
            .unwrap();
        assert!(on_disk.contains("PodmanArgs=--memory=256m"));
    }

    #[tokio::test]
    async fn write_if_changed_atomic_rename_leaves_no_tmp() {
        let dir = tempdir().unwrap();
        write_if_changed(&sample_unit(), dir.path()).await.unwrap();
        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        while let Some(e) = entries.next_entry().await.unwrap() {
            assert!(
                !e.file_name().to_string_lossy().ends_with(".tmp"),
                "atomic rename must leave no .tmp residue"
            );
        }
    }

    // ────────────────────────────────────────────────────────────────
    // Phase 3.1 backend renderer tests
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn parse_memory_mib_recognises_common_forms() {
        assert_eq!(parse_memory_mib("1024"), Some(1024));
        assert_eq!(parse_memory_mib("512m"), Some(512));
        assert_eq!(parse_memory_mib("512M"), Some(512));
        assert_eq!(parse_memory_mib("2g"), Some(2048));
        assert_eq!(parse_memory_mib("2G"), Some(2048));
        assert_eq!(parse_memory_mib("1k"), None); // sub-MiB rejected
        assert_eq!(parse_memory_mib("garbage"), None);
        assert_eq!(parse_memory_mib(""), None);
        assert_eq!(parse_memory_mib(" 256m "), Some(256));
    }

    #[test]
    fn shell_join_quotes_only_when_needed() {
        assert_eq!(shell_join(&["bitcoind".into()]), "bitcoind");
        assert_eq!(
            shell_join(&["bitcoind".into(), "-server=1".into()]),
            "bitcoind -server=1"
        );
        // Whitespace forces quoting:
        assert_eq!(
            shell_join(&["bash".into(), "-c".into(), "echo hi".into()]),
            "bash -c \"echo hi\""
        );
        // Embedded quotes must escape:
        assert_eq!(shell_join(&[r#"say "hi""#.into()]), r#""say \"hi\"""#);
        assert_eq!(
            shell_join(&[
                "sh".into(),
                "-lc".into(),
                "if true; then\n  exec app;\nfi".into()
            ]),
            "sh -lc \"if true; then   exec app; fi\""
        );
    }

    #[test]
    fn apostrophes_survive_quadlet_argument_and_environment_parsing() {
        // A whitespace-free Node script reproduced this in a real Quadlet:
        // unquoted apostrophes were consumed by the parser, changing JS strings
        // into identifiers and preventing the app from starting.
        assert_eq!(
            shell_join(&["require('http')".into()]),
            "\"require('http')\""
        );
        assert_eq!(quote_environment("NAME=O'Brien"), "\"NAME=O'Brien\"");
    }

    #[test]
    fn quote_environment_quotes_values_with_spaces() {
        assert_eq!(
            quote_environment("BITCOIN_RPC_PASS=secret"),
            "BITCOIN_RPC_PASS=secret"
        );
        assert_eq!(
            quote_environment("RELAY_NAME=Archipelago Nostr Relay"),
            "\"RELAY_NAME=Archipelago Nostr Relay\""
        );
        assert_eq!(
            quote_environment("GREETING=say \"hi\""),
            "\"GREETING=say \\\"hi\\\"\""
        );
    }

    #[test]
    fn percent_is_escaped_for_systemd_specifiers() {
        // Regression: a bare `%s` in an Exec= line is a systemd specifier
        // (user's shell = /bin/bash) once quadlet copies it into the
        // generated ExecStart — bitcoind's rpc.conf came out as
        // `rpcuser=/bin/bash` and every RPC consumer got 401s.
        assert_eq!(
            shell_join(&["printf 'rpcuser=%s' \"$U\"".to_string()]),
            "\"printf 'rpcuser=%%s' \\\"$$U\\\"\""
        );
        assert_eq!(shell_join(&["--fmt=%h:%p".to_string()]), "--fmt=%%h:%%p");
        assert_eq!(quote_environment("FMT=%m"), "FMT=%%m");
    }

    #[test]
    fn restart_policy_emits_correct_systemd_string() {
        assert_eq!(RestartPolicy::Always.as_systemd(), "always");
        assert_eq!(RestartPolicy::OnFailure.as_systemd(), "on-failure");
    }

    #[test]
    fn render_bound_ports_prefix_the_bind_address() {
        // Bitcoin RPC hardening: the same host port published on loopback +
        // the archy-net gateway; an unbound port must render byte-identical
        // to the legacy form so existing units don't read as drifted.
        let u = QuadletUnit {
            name: "bitcoin-knots".into(),
            image: "registry/bitcoin-knots:latest".into(),
            network: NetworkMode::Bridge("archy-net".into()),
            ports: vec![
                (8332, 8332, "tcp".into(), "127.0.0.1".into()),
                (8332, 8332, "tcp".into(), "10.89.0.1".into()),
                (8333, 8333, "tcp".into(), String::new()),
            ],
            ..QuadletUnit::default()
        };
        let s = u.render();
        assert!(s.contains("PublishPort=127.0.0.1:8332:8332/tcp"));
        assert!(s.contains("PublishPort=10.89.0.1:8332:8332/tcp"));
        assert!(s.contains("PublishPort=0.0.0.0:8333:8333/tcp"));
        assert!(!s.contains("PublishPort=0.0.0.0:8332:8332/tcp"));
    }

    #[test]
    fn render_emits_backend_directives_when_set() {
        let u = QuadletUnit {
            name: "bitcoin-knots".into(),
            description: "Bitcoin Knots backend".into(),
            image: "registry/bitcoin-knots:latest".into(),
            network: NetworkMode::Bridge("archy-net".into()),
            cap_drop_all: true,
            cap_add: vec!["NET_BIND_SERVICE".into()],
            ports: vec![
                (8332, 8332, "tcp".into(), String::new()),
                (8333, 8333, "tcp".into(), String::new()),
            ],
            environment: vec![
                "BITCOIN_RPC_USER=archipelago".into(),
                "BITCOIN_RPC_PASS=secret".into(),
                "RELAY_NAME=Archipelago Nostr Relay".into(),
            ],
            devices: vec!["/dev/kvm".into()],
            add_hosts: vec![("host.archipelago".into(), "10.89.0.1".into())],
            entrypoint: Some(vec!["/usr/local/bin/bitcoind".into()]),
            command: vec!["-server=1".into(), "-rpcbind=0.0.0.0".into()],
            read_only_root: true,
            no_new_privileges: true,
            cpu_quota: Some(2),
            restart_policy: RestartPolicy::OnFailure,
            ..QuadletUnit::default()
        };
        let s = u.render();
        assert!(s.contains("PublishPort=0.0.0.0:8332:8332/tcp"));
        assert!(s.contains("PublishPort=0.0.0.0:8333:8333/tcp"));
        assert!(s.contains("Environment=BITCOIN_RPC_USER=archipelago"));
        assert!(s.contains("Environment=BITCOIN_RPC_PASS=secret"));
        assert!(s.contains("Environment=\"RELAY_NAME=Archipelago Nostr Relay\""));
        assert!(s.contains("AddDevice=/dev/kvm"));
        assert!(s.contains("AddHost=host.archipelago:10.89.0.1"));
        assert!(s.contains("ReadOnly=true"));
        assert!(s.contains("NoNewPrivileges=true"));
        assert!(s.contains("PodmanArgs=--cpus=2"));
        // Manifest entrypoint becomes a real Entrypoint= override (not folded
        // into Exec=), so the unit doesn't depend on the image's own ENTRYPOINT.
        assert!(s.contains("Entrypoint=/usr/local/bin/bitcoind"));
        assert!(s.contains("Exec=-server=1 -rpcbind=0.0.0.0"));
        assert!(s.contains("Restart=on-failure"));
        assert!(s.contains("Network=archy-net"));
    }

    #[test]
    fn render_skips_backend_directives_when_default() {
        // Companion-style unit: backend extension fields all defaulted.
        // Rendered bytes must not include any of the backend directives,
        // so existing companion units stay byte-identical to before.
        let s = sample_unit().render();
        assert!(!s.contains("PublishPort="));
        assert!(!s.contains("Environment="));
        assert!(!s.contains("AddDevice="));
        assert!(!s.contains("AddHost="));
        assert!(!s.contains("ReadOnly="));
        assert!(!s.contains("NoNewPrivileges="));
        assert!(!s.contains("Exec="));
        assert!(!s.contains("--cpus="));
        // Default RestartPolicy is Always — companions rely on this.
        assert!(s.contains("Restart=always"));
    }

    #[test]
    fn from_manifest_translates_a_typical_backend() {
        let yaml = r#"
app:
  id: bitcoin-knots
  name: Bitcoin Knots
  version: 1.0.0
  container:
    image: registry/bitcoin-knots:1.0
    network: archy-net
    entrypoint: ["/usr/local/bin/bitcoind"]
    custom_args: ["-server=1", "-rpcbind=0.0.0.0"]
  ports:
    - host: 8332
      container: 8332
      protocol: tcp
  volumes:
    - type: bind
      source: /var/lib/archipelago/bitcoin
      target: /home/bitcoin/.bitcoin
      options: []
  environment:
    - BITCOIN_NETWORK=mainnet
  devices: []
  resources:
    cpu_limit: 4
    memory_limit: 2g
  security:
    capabilities: ["NET_BIND_SERVICE"]
    readonly_root: true
    network_policy: isolated
"#;
        let m = AppManifest::parse(yaml).expect("manifest must parse");
        let u = QuadletUnit::from_manifest(&m, "bitcoin-knots");
        assert_eq!(u.name, "bitcoin-knots");
        assert_eq!(u.image, "registry/bitcoin-knots:1.0");
        assert!(matches!(u.network, NetworkMode::Bridge(ref n) if n == "archy-net"));
        assert_eq!(u.memory_mb, Some(2048));
        assert_eq!(u.cpu_quota, Some(4));
        assert!(u.read_only_root);
        assert!(u.no_new_privileges);
        assert_eq!(u.cap_add, vec!["NET_BIND_SERVICE"]);
        assert_eq!(
            u.ports,
            vec![(8332, 8332, "tcp".to_string(), String::new())]
        );
        assert_eq!(u.environment, vec!["BITCOIN_NETWORK=mainnet"]);
        assert_eq!(u.bind_mounts.len(), 1);
        assert_eq!(
            u.bind_mounts[0].host,
            PathBuf::from("/var/lib/archipelago/bitcoin")
        );
        assert!(!u.bind_mounts[0].read_only);
        assert_eq!(u.entrypoint, Some(vec!["/usr/local/bin/bitcoind".into()]));
        assert_eq!(u.command, vec!["-server=1", "-rpcbind=0.0.0.0"]);
        assert!(u
            .add_hosts
            .iter()
            .any(|(n, ip)| n == "host.archipelago" && ip == "10.89.0.1"));
        // Always, not OnFailure. Quadlet renders `--rm`, so OnFailure left a
        // cleanly-exited app both deleted and unrestarted — the vanishing
        // bitcoin-knots/electrumx bug. Do not relax this back.
        assert_eq!(u.restart_policy, RestartPolicy::Always);
    }

    #[test]
    fn from_manifest_uses_default_network_for_isolated_ports() {
        let yaml = r#"
app:
  id: searxng
  name: SearXNG
  version: 1.0.0
  container:
    image: searxng:latest
  ports:
    - host: 8888
      container: 8080
      protocol: tcp
  security:
    network_policy: isolated
"#;
        let m = AppManifest::parse(yaml).expect("manifest must parse");
        let s = QuadletUnit::from_manifest(&m, "searxng").render();

        assert!(s.contains("PublishPort=0.0.0.0:8888:8080/tcp"));
        assert!(!s.contains("Network=host"));
    }

    #[test]
    fn portainer_catalog_network_repairs_same_node_routing_without_exposing_backend() {
        let manifest = AppManifest::parse(include_str!("../../../../apps/portainer/manifest.yml"))
            .expect("shipped Portainer manifest must parse");
        let new = QuadletUnit::from_manifest(&manifest, "portainer").render();
        assert!(new.contains("Network=slirp4netns\n"));
        assert!(!new.contains("NetworkAlias="));
        assert!(new.contains("PublishPort=127.0.0.1:9000:9000/tcp"));
        assert!(!new.contains("PublishPort=0.0.0.0"));
        // The upgrade changes networking only: retain both state mounts and the
        // existing rootless socket, without an app.ini or repository rewrite.
        assert!(new.contains("Volume=/var/lib/archipelago/portainer:/data"));
        assert!(new.contains("Volume=/var/lib/archipelago/portainer/compose:/data/compose"));
        assert!(new.contains("Volume=/run/user/1000/podman/podman.sock:/var/run/docker.sock"));
        let old = new.replace("Network=slirp4netns\n", "");
        assert!(network_aliases_changed(&old, &new));
        assert!(!network_aliases_changed(&new, &new));
        assert!(!publish_ports_changed(&old, &new));
    }

    #[test]
    fn from_manifest_slirp4netns_omits_network_alias() {
        let yaml = r#"
app:
  id: vaultwarden
  name: Vaultwarden
  version: 1.0.0
  container:
    image: registry/vaultwarden:1
    network: slirp4netns
  security:
    network_policy: isolated
"#;
        let m = AppManifest::parse(yaml).expect("manifest must parse");
        let s = QuadletUnit::from_manifest(&m, "vaultwarden").render();

        assert!(s.contains("Network=slirp4netns"));
        assert!(!s.contains("NetworkAlias="));
        assert!(!s.contains("--network-alias"));
    }

    #[test]
    fn from_manifest_pasta_omits_network_alias() {
        let yaml = r#"
app:
  id: nextcloud
  name: Nextcloud
  version: 1.0.0
  container:
    image: registry/nextcloud:1
    network: pasta
  security:
    network_policy: isolated
"#;
        let m = AppManifest::parse(yaml).expect("manifest must parse");
        let s = QuadletUnit::from_manifest(&m, "nextcloud").render();

        assert!(s.contains("Network=pasta"));
        assert!(!s.contains("NetworkAlias="));
        assert!(!s.contains("--network-alias"));
    }

    #[test]
    fn from_manifest_preserves_grafana_data_uid_and_volume_shape() {
        let yaml = r#"
app:
  id: grafana
  name: Grafana
  version: 10.2.0
  container:
    image: grafana/grafana:10.2.0
    data_uid: "472:472"
  volumes:
    - type: bind
      source: /var/lib/archipelago/grafana
      target: /var/lib/grafana
      options: [rw]
  resources:
    memory_limit: 1g
"#;
        let m = AppManifest::parse(yaml).unwrap();
        assert_eq!(m.app.container.data_uid.as_deref(), Some("472:472"));

        let u = QuadletUnit::from_manifest(&m, "grafana");
        assert_eq!(u.memory_mb, Some(1024));
        assert_eq!(u.bind_mounts.len(), 1);
        assert_eq!(
            u.bind_mounts[0].host,
            PathBuf::from("/var/lib/archipelago/grafana")
        );
        assert_eq!(
            u.bind_mounts[0].container,
            PathBuf::from("/var/lib/grafana")
        );
        assert!(!u.bind_mounts[0].read_only);
    }

    #[test]
    fn from_manifest_marks_ro_volumes_read_only() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    image: x:latest
  volumes:
    - type: bind
      source: /var/lib/archipelago/x-conf
      target: /etc/conf
      options: ["ro"]
"#;
        let m = AppManifest::parse(yaml).unwrap();
        let u = QuadletUnit::from_manifest(&m, "x");
        assert_eq!(u.bind_mounts.len(), 1);
        assert!(u.bind_mounts[0].read_only);
    }

    #[test]
    fn from_manifest_skips_tmpfs_volumes() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    image: x:latest
  volumes:
    - type: tmpfs
      target: /tmp
      tmpfs_options: "rw,size=64m"
    - type: bind
      source: /var/lib/archipelago/x
      target: /data
      options: []
"#;
        let m = AppManifest::parse(yaml).unwrap();
        let u = QuadletUnit::from_manifest(&m, "x");
        // tmpfs entry is dropped from bind_mounts; bind entry survives.
        assert_eq!(u.bind_mounts.len(), 1);
        assert_eq!(
            u.bind_mounts[0].host,
            PathBuf::from("/var/lib/archipelago/x")
        );
    }

    #[test]
    fn render_emits_health_directives_when_set() {
        let mut u = QuadletUnit::default();
        u.name = "lnd".into();
        u.image = "x:1".into();
        u.health = Some(HealthSpec {
            cmd: "probe-ready".into(),
            interval: "30s".into(),
            timeout: "5s".into(),
            retries: 3,
        });
        let s = u.render();
        assert!(s.contains("HealthCmd=probe-ready"));
        assert!(s.contains("HealthInterval=30s"));
        assert!(s.contains("HealthTimeout=5s"));
        assert!(s.contains("HealthRetries=3"));
        assert!(!s.contains("Notify=healthy"));
        assert!(!s.contains("TimeoutStartSec=600"));
        assert!(s.contains("TimeoutStartSec=0"));
    }

    #[test]
    fn render_skips_health_directives_when_absent() {
        // No health spec → no Notify=healthy and no HealthCmd. TimeoutStartSec=0
        // is a service-level baseline so dependency-waiting apps are not killed
        // by systemd before their app daemon binds.
        let s = sample_unit().render();
        assert!(!s.contains("HealthCmd="));
        assert!(!s.contains("Notify=healthy"));
        assert!(!s.contains("HealthRetries="));
        assert!(s.contains("TimeoutStartSec=0"));
        assert!(!s.contains("TimeoutStartSec=600"));
    }

    #[test]
    fn translate_health_check_handles_each_supported_type() {
        use archipelago_container::HealthCheck;
        let tcp = HealthCheck {
            check_type: "tcp".into(),
            endpoint: Some("localhost:10009".into()),
            path: None,
            interval: "30s".into(),
            timeout: "5s".into(),
            retries: 3,
        };
        assert!(translate_health_check(&tcp).is_none());

        let http = HealthCheck {
            check_type: "http".into(),
            endpoint: Some("localhost:8080".into()),
            path: Some("/health".into()),
            interval: "10s".into(),
            timeout: "3s".into(),
            retries: 5,
        };
        let h = translate_health_check(&http).expect("http must translate");
        assert_eq!(
            h.cmd,
            "if command -v wget >/dev/null 2>&1; then wget -q -T 3 -O /dev/null http://localhost:8080/health; elif command -v curl >/dev/null 2>&1; then curl -fsS -m 3 http://localhost:8080/health; elif command -v bash >/dev/null 2>&1; then bash -c 'exec 3<>/dev/tcp/localhost/8080'; else exit 0; fi"
        );

        // The /dev/tcp fallback must target the URL's host:port, with
        // scheme-default ports when the authority carries none.
        assert_eq!(
            health_url_host_port("http://localhost:8080/health"),
            ("localhost".to_string(), 8080)
        );
        assert_eq!(
            health_url_host_port("http://127.0.0.1/"),
            ("127.0.0.1".to_string(), 80)
        );
        assert_eq!(
            health_url_host_port("https://localhost/status"),
            ("localhost".to_string(), 443)
        );

        let cmdck = HealthCheck {
            check_type: "cmd".into(),
            endpoint: Some("/usr/local/bin/probe.sh".into()),
            path: None,
            interval: "60s".into(),
            timeout: "15s".into(),
            retries: 2,
        };
        let h = translate_health_check(&cmdck).expect("cmd must translate");
        assert_eq!(h.cmd, "/usr/local/bin/probe.sh");

        // Unknown type → None rather than emit a broken HealthCmd.
        let bad = HealthCheck {
            check_type: "exec".into(),
            endpoint: Some("foo".into()),
            path: None,
            interval: "30s".into(),
            timeout: "5s".into(),
            retries: 3,
        };
        assert!(translate_health_check(&bad).is_none());

        // TCP is skipped entirely for Quadlet units.
        let badtcp = HealthCheck {
            check_type: "tcp".into(),
            endpoint: Some("hostonly".into()),
            path: None,
            interval: "30s".into(),
            timeout: "5s".into(),
            retries: 3,
        };
        assert!(translate_health_check(&badtcp).is_none());
    }

    #[test]
    fn translate_health_check_http_does_not_double_prefix_scheme() {
        // Regression: on .228 2026-05-02 we shipped HealthCmds reading
        // `curl -fsS -m 5 http://http://localhost:8175/` because manifests
        // in the wild carry the scheme inside the endpoint string. Every
        // probe failed and the unit looped. Now we accept either form.
        use archipelago_container::HealthCheck;
        let with_scheme = HealthCheck {
            check_type: "http".into(),
            endpoint: Some("http://localhost:8175".into()),
            path: Some("/".into()),
            interval: "30s".into(),
            timeout: "5s".into(),
            retries: 3,
        };
        let h = translate_health_check(&with_scheme).expect("with-scheme must translate");
        assert!(h.cmd.contains("http://localhost:8175/"));
        assert!(!h.cmd.contains("http://http://"), "got: {}", h.cmd);

        let with_https = HealthCheck {
            check_type: "http".into(),
            endpoint: Some("https://example.local/health".into()),
            path: None,
            interval: "30s".into(),
            timeout: "5s".into(),
            retries: 3,
        };
        let h = translate_health_check(&with_https).expect("https must translate");
        // Endpoint already has /health → don't append the default "/".
        assert!(h.cmd.contains("https://example.local/health"));
    }

    #[test]
    fn translate_health_check_http_uses_manifest_timeout_for_helpers() {
        use archipelago_container::HealthCheck;
        let http = HealthCheck {
            check_type: "http".into(),
            endpoint: Some("localhost:3000".into()),
            path: Some("/api/health".into()),
            interval: "30s".into(),
            timeout: "30s".into(),
            retries: 5,
        };

        let h = translate_health_check(&http).expect("http must translate");
        assert!(h.cmd.contains("wget -q -T 30 "), "got: {}", h.cmd);
        assert!(h.cmd.contains("curl -fsS -m 30 "), "got: {}", h.cmd);
        assert_eq!(h.timeout, "30s");
        assert_eq!(h.retries, 5);
    }

    #[test]
    fn from_manifest_picks_up_health_check() {
        let yaml = r#"
app:
  id: lnd
  name: LND
  version: 1.0.0
  container:
    image: x:1
  health_check:
    type: tcp
    endpoint: localhost:10009
    interval: 15s
    timeout: 4s
    retries: 5
"#;
        let m = AppManifest::parse(yaml).unwrap();
        let u = QuadletUnit::from_manifest(&m, "lnd");
        assert!(u.health.is_none());
        assert!(!u.render().contains("Notify=healthy"));
    }

    #[test]
    fn publish_ports_changed_detects_port_binding_drift() {
        let old = "[Container]\nPublishPort=9735:9735/tcp\nPublishPort=8080:8080/tcp\n";
        let new = "[Container]\nPublishPort=9735:9735/tcp\nPublishPort=18080:8080/tcp\n";
        assert!(publish_ports_changed(old, new));
        assert!(!publish_ports_changed(new, new));
    }

    #[test]
    fn from_manifest_appends_manifest_network_aliases_for_bridge() {
        let yaml = r#"
app:
  id: indeedhub-api
  name: IndeedHub API
  version: 1.0.0
  container:
    image: registry/indeedhub-api:1.0.0
    network: indeedhub-net
    network_aliases: [api]
  security:
    capabilities: []
    network_policy: isolated
"#;
        let m = AppManifest::parse(yaml).expect("manifest must parse");
        let u = QuadletUnit::from_manifest(&m, "indeedhub-api");
        assert!(matches!(u.network, NetworkMode::Bridge(ref n) if n == "indeedhub-net"));
        // Own name first, then the baked-in short alias the frontend nginx uses.
        assert_eq!(u.network_aliases, vec!["indeedhub-api", "api"]);
        let s = u.render();
        assert!(s.contains("NetworkAlias=api"));
        assert!(s.contains("PodmanArgs=--network-alias=api"));
    }

    #[test]
    fn network_aliases_changed_detects_service_discovery_drift() {
        let old = "[Container]\nNetwork=archy-net\n";
        let new = "[Container]\nNetwork=archy-net\nNetworkAlias=bitcoin-knots\n";
        assert!(network_aliases_changed(old, new));
        assert!(!network_aliases_changed(new, new));
    }

    #[tokio::test]
    async fn failed_runtime_change_remains_pending_when_unit_already_matches() {
        let dir = tempfile::tempdir().unwrap();
        let unit = dir.path().join("portainer.container");
        tokio::fs::write(&unit, "[Container]\n").await.unwrap();
        let pending = RestartObligation::prepare(&unit, true).await.unwrap();
        assert!(pending.is_pending());
        tokio::fs::write(&unit, "[Container]\nNetwork=slirp4netns\n")
            .await
            .unwrap();
        // Simulate systemctl failure or daemon interruption after unit rewrite.
        drop(pending);
        let retry = RestartObligation::prepare(&unit, false).await.unwrap();
        assert!(
            retry.is_pending(),
            "matching unit must not discard failed restart"
        );
        retry.complete().await.unwrap();
        assert!(!RestartObligation::prepare(&unit, false)
            .await
            .unwrap()
            .is_pending());
    }

    #[tokio::test]
    async fn pending_runtime_change_errors_are_not_reported_as_success() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing/app.container");
        assert!(RestartObligation::prepare(&missing, true).await.is_err());
        let unit = dir.path().join("app.container");
        let pending = RestartObligation::prepare(&unit, true).await.unwrap();
        tokio::fs::remove_file(unit.with_extension("restart-pending"))
            .await
            .unwrap();
        assert!(pending.complete().await.is_err());
    }

    #[test]
    fn gitea_ssh_sandbox_capability_is_applied_as_a_runtime_change() {
        let manifest =
            AppManifest::parse(include_str!("../../../../apps/gitea/manifest.yml")).unwrap();
        manifest.validate().unwrap();
        let new = QuadletUnit::from_manifest(&manifest, "gitea").render();
        assert!(new.contains("AddCapability=SYS_CHROOT\n"));
        let old = new.replace("AddCapability=SYS_CHROOT\n", "");
        assert!(security_changed(&old, &new));
        assert!(!security_changed(&new, &new));
        assert!(!security_changed(
            "AddCapability=CHOWN\nAddCapability=SETUID\n",
            "AddCapability=SETUID\nAddCapability=CHOWN\n"
        ));
    }

    #[test]
    fn network_aliases_changed_detects_network_mode_drift() {
        let old = "[Container]\nNetwork=slirp4netns\n";
        let new = "[Container]\n";
        assert!(network_aliases_changed(old, new));
        assert!(!network_aliases_changed(new, new));
    }

    #[test]
    fn shell_join_escapes_dollars_for_container_runtime_expansion() {
        let rendered = shell_join(&["sh".into(), "-lc".into(), "echo ${BITCOIN_RPC_PASS}".into()]);
        assert!(rendered.contains("$${BITCOIN_RPC_PASS}"));
    }

    #[test]
    fn exec_changed_detects_command_drift() {
        let old = "[Container]\nExec=sh -lc \"echo ${BITCOIN_RPC_PASS}\"\n";
        let new = "[Container]\nExec=sh -lc \"echo $${BITCOIN_RPC_PASS}\"\n";
        assert!(exec_changed(old, new));
        assert!(!exec_changed(new, new));
    }

    #[test]
    fn health_cmd_changed_detects_probe_drift() {
        let old = "[Container]\nHealthCmd=curl -fsS http://localhost:8080/\n";
        let new = "[Container]\nHealthCmd=if command -v wget >/dev/null 2>&1; then wget -q -T 5 -O /dev/null http://localhost:8080/; elif command -v curl >/dev/null 2>&1; then curl -fsS -m 5 http://localhost:8080/; else exit 0; fi\n";
        assert!(health_cmd_changed(old, new));
        assert!(!health_cmd_changed(new, new));
    }

    #[test]
    fn health_cmd_changed_detects_probe_timing_drift() {
        let old = "[Container]\nHealthCmd=curl -fsS http://localhost:8080/\nHealthTimeout=5s\nHealthRetries=3\n";
        let new = "[Container]\nHealthCmd=curl -fsS http://localhost:8080/\nHealthTimeout=30s\nHealthRetries=5\n";
        assert!(health_cmd_changed(old, new));
        assert!(!health_cmd_changed(new, new));
    }

    #[test]
    fn from_manifest_renders_to_a_systemd_unit() {
        // End-to-end: parse a real-shape manifest, build the unit, render
        // the bytes, and assert the unit body contains the directives a
        // human would write by hand.
        let yaml = r#"
app:
  id: lnd
  name: LND
  version: 1.0.0
  container:
    image: registry/lnd:latest
    network: archy-net
  ports:
    - host: 10009
      container: 10009
      protocol: tcp
  volumes:
    - type: bind
      source: /var/lib/archipelago/lnd
      target: /root/.lnd
      options: []
  environment:
    - LND_NETWORK=mainnet
  resources:
    memory_limit: 1g
  security:
    capabilities: []
    network_policy: isolated
"#;
        let m = AppManifest::parse(yaml).unwrap();
        let body = QuadletUnit::from_manifest(&m, "lnd").render();
        assert!(body.contains("ContainerName=lnd"));
        assert!(body.contains("Image=registry/lnd:latest"));
        assert!(body.contains("Network=archy-net"));
        assert!(body.contains("NetworkAlias=lnd"));
        assert!(body.contains("PodmanArgs=--network-alias=lnd"));
        assert!(body.contains("PublishPort=0.0.0.0:10009:10009/tcp"));
        assert!(body.contains("Volume=/var/lib/archipelago/lnd:/root/.lnd:Z"));
        assert!(body.contains("Environment=LND_NETWORK=mainnet"));
        assert!(body.contains("PodmanArgs=--memory=1024m"));
        assert!(body.contains("AddHost=host.archipelago:10.89.0.1"));
        assert!(body.contains("DropCapability=ALL"));
        assert!(body.contains("NoNewPrivileges=true"));
        // A rendered backend unit must never carry on-failure again: paired
        // with quadlet's `--rm` it deletes a cleanly-stopped app and leaves it
        // deleted.
        assert!(body.contains("Restart=always"));
        assert!(!body.contains("Restart=on-failure"));
    }
}
