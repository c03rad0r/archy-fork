use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("Invalid manifest: {0}")]
    Invalid(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    pub app: AppDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDefinition {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,

    #[serde(default)]
    pub container: ContainerConfig,

    #[serde(default)]
    pub dependencies: Vec<Dependency>,

    #[serde(default)]
    pub resources: ResourceLimits,

    #[serde(default)]
    pub security: SecurityPolicy,

    #[serde(default)]
    pub ports: Vec<PortMapping>,

    #[serde(default)]
    pub volumes: Vec<Volume>,

    #[serde(default)]
    pub files: Vec<GeneratedFile>,

    #[serde(default)]
    pub environment: Vec<String>,

    #[serde(default)]
    pub health_check: Option<HealthCheck>,

    #[serde(default)]
    pub devices: Vec<String>,

    #[serde(default)]
    pub interfaces: HashMap<String, AppInterface>,

    /// Controlled post-install / pre-start lifecycle hooks. Declarative,
    /// allowlisted operations run against the app's OWN container — never the
    /// host. See `docs/manifest-hooks-design.md`.
    #[serde(default)]
    pub hooks: LifecycleHooks,

    #[serde(flatten)]
    pub extensions: HashMap<String, serde_yaml::Value>,
}

/// Declarative lifecycle hooks for an app. Absent = none (forward-compatible).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleHooks {
    /// Run once after a successful install, with the container created + running.
    #[serde(default)]
    pub post_install: Vec<HookStep>,
    /// Run before each start (repair/ownership). Reserved; not yet executed.
    #[serde(default)]
    pub pre_start: Vec<HookStep>,
}

/// A single controlled hook operation. Each list item is a one-key map, e.g.
/// `- exec: [...]` or `- copy_from_host: { src, dest }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum HookStep {
    /// Run a command vector INSIDE the app's container (`podman exec`). Never on
    /// the host; inherits the container's (already dropped) capabilities.
    Exec { exec: Vec<String> },
    /// Copy a file from an allowlisted host root into the container. `src` is
    /// relative to the allowlist (data dir / web-ui) — no absolute paths, no `..`.
    CopyFromHost {
        #[serde(rename = "copy_from_host")]
        copy_from_host: HostCopy,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostCopy {
    pub src: String,
    pub dest: String,
}

impl LifecycleHooks {
    fn validate(&self) -> Result<(), ManifestError> {
        for step in self.post_install.iter().chain(self.pre_start.iter()) {
            step.validate()?;
        }
        Ok(())
    }
}

impl HookStep {
    fn validate(&self) -> Result<(), ManifestError> {
        match self {
            HookStep::Exec { exec } => {
                if exec.is_empty() {
                    return Err(ManifestError::Invalid(
                        "hooks: exec must be a non-empty command vector".to_string(),
                    ));
                }
            }
            HookStep::CopyFromHost { copy_from_host } => {
                let s = &copy_from_host.src;
                if s.is_empty() || s.starts_with('/') || s.contains("..") {
                    return Err(ManifestError::Invalid(format!(
                        "hooks: copy_from_host.src must be a relative allowlisted path \
                         (no leading '/', no '..'), got '{s}'"
                    )));
                }
                if copy_from_host.dest.is_empty() || !copy_from_host.dest.starts_with('/') {
                    return Err(ManifestError::Invalid(format!(
                        "hooks: copy_from_host.dest must be an absolute container path, got '{}'",
                        copy_from_host.dest
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContainerConfig {
    /// Pull source. Mutually exclusive with `build`. Exactly one of the two must be present.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_signature: Option<String>,
    #[serde(default = "default_pull_policy")]
    pub pull_policy: String,
    /// Local build source. Mutually exclusive with `image`.
    #[serde(default)]
    pub build: Option<BuildConfig>,

    // ── Step 8b.0 additions ──────────────────────────────────────────
    //
    // Fields the Rust orchestrator needs to faithfully port containers
    // from the legacy `scripts/container-specs.sh` registry. See
    // `docs/STEP-8B-PORT-AUDIT.md` for the full justification per field.
    //
    // All are optional with `#[serde(default)]` so every existing manifest
    // in `apps/` continues to parse unchanged.
    /// Podman `--network` value. `Some("archy-net")` joins the shared
    /// Archipelago bridge. `Some("host")` uses host networking.
    /// `None` (the default) falls back to podman's default isolated
    /// network — equivalent to today's rootless default.
    ///
    /// `SecurityPolicy::network_policy` remains a policy knob (what the
    /// firewall layer does); this field is literally the CLI flag value.
    #[serde(default)]
    pub network: Option<String>,

    /// Extra DNS aliases the container answers to on its `network`, in addition
    /// to its own container name (which is always added). Mirrors podman
    /// `--network-alias`. Used by multi-container stacks whose images reference
    /// peers by a short baked-in hostname — e.g. indeedhub's frontend nginx
    /// proxies to `api:4000` / `minio:9000` / `relay:8080`, so the api/minio/relay
    /// members declare `network_aliases: [api]` / `[minio]` / `[relay]` to keep
    /// those short names resolvable on the dedicated `indeedhub-net`. Ignored for
    /// slirp4netns/pasta (podman rejects aliases there).
    #[serde(default)]
    pub network_aliases: Vec<String>,

    /// Extra positional arguments appended to the container command
    /// after the image. Mirrors `SPEC_CUSTOM_ARGS` in
    /// `scripts/container-specs.sh` (bitcoin-knots prune/dbcache flags,
    /// filebrowser `--config /data/.filebrowser.json`, etc).
    #[serde(default)]
    pub custom_args: Vec<String>,

    /// Entrypoint override (`podman run --entrypoint …`). When present,
    /// replaces the image's default entrypoint. Mirrors `SPEC_ENTRYPOINT`
    /// for fedimint-gateway's LND-aware invocation.
    #[serde(default)]
    pub entrypoint: Option<Vec<String>>,

    /// Environment keys whose values are rendered from a small
    /// allow-list of host facts (`HOST_IP`, `HOST_MDNS`, `DISK_GB`).
    /// Resolved by `ContainerConfig::resolve_derived_env` at apply time
    /// — never hard-coded into the manifest.
    ///
    /// Example: `- { key: FM_P2P_URL, template: "fedimint://{{HOST_MDNS}}:8173" }`
    #[serde(default)]
    pub derived_env: Vec<DerivedEnv>,

    /// Environment keys whose values are read from files in
    /// `/var/lib/archipelago/secrets/<secret_file>`. Never logged.
    /// Resolved by `ContainerConfig::resolve_secret_env` at apply time.
    ///
    /// Example: `- { key: FM_BITCOIND_PASSWORD, secret_file: bitcoin-rpc-password }`
    #[serde(default)]
    pub secret_env: Vec<SecretEnv>,

    /// Secrets the orchestrator generates on first use when absent, so an app
    /// installs from its manifest alone — no host provisioning, no per-app Rust.
    /// Materialised before `secret_env` is resolved, written `0600` and owned by
    /// the unprivileged (rootless) service user. Idempotent and self-healing: a
    /// file that already exists and is readable is left untouched; one that is
    /// present-but-unreadable (e.g. wrongly created `root`-owned) is recreated
    /// in place via the service-owned secrets dir — no `chown`, no privilege.
    ///
    /// Example: `- { name: fmcd-password, kind: hex16 }`
    #[serde(default)]
    pub generated_secrets: Vec<GeneratedSecret>,

    /// Self-signed TLS certificates the orchestrator materialises before the
    /// container is created (so a bind-mounted cert path resolves to a real
    /// file, not a stale/missing path). Like `generated_secrets`, this keeps an
    /// app data-driven: a service that needs a secure context (e.g. netbird's
    /// dashboard — OIDC PKCE / `window.crypto.subtle` only works over HTTPS,
    /// issue #15) declares the cert here instead of relying on per-app Rust.
    /// Idempotent: an entry whose `crt` and `key` already exist is left
    /// untouched. SAN/CN templates are rendered against host facts at apply time.
    ///
    /// Example: `- { crt: /var/lib/archipelago/netbird/tls.crt, key: /var/lib/archipelago/netbird/tls.key }`
    #[serde(default)]
    pub generated_certs: Vec<GeneratedCert>,

    /// Rootless-mapped UID:GID applied to the container's data directory
    /// (the `bind`-mounted host path with `target` inside the container's
    /// data root) before creation. Mirrors `SPEC_DATA_UID`.
    ///
    /// Example: `"100070:100070"` for Postgres' mapped subuid.
    #[serde(default)]
    pub data_uid: Option<String>,

    /// Runtime-resolved secret env entries (never serialized, never in a
    /// manifest file). Populated by the orchestrator's env-resolution
    /// chokepoint; the backends turn each ref into a podman secret
    /// reference (`secret_env` in the API spec / `Secret=…,type=env` in
    /// Quadlet) so the VALUE never lands in `podman inspect` output or a
    /// unit file on disk. The dev-only docker fallback injects `value` as
    /// plain env — docker has no rootless secret store.
    #[serde(skip)]
    pub secret_env_refs: Vec<SecretEnvRef>,

    /// sha256 over all resolved secret env pairs, stamped onto the
    /// container as a label so rotation is detectable as drift without
    /// exposing values. None when the app has no secret env.
    #[serde(skip)]
    pub secret_env_hash: Option<String>,
}

/// Derived-env entry. The template is rendered against `HostFacts` at
/// apply time; exactly one `{{PLACEHOLDER}}` occurrence per supported
/// fact name is allowed (host_ip, host_mdns, disk_gb).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedEnv {
    pub key: String,
    pub template: String,
}

/// Secret-env entry. `secret_file` is resolved against a
/// `SecretsProvider` (in prod, `/var/lib/archipelago/secrets/`).
///
/// `secret_file` is restricted to a bare filename — no `/`, no `..`.
/// Validated at `AppManifest::validate` time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretEnv {
    pub key: String,
    pub secret_file: String,
    /// When true, a missing/unreadable/empty secret skips this entry instead
    /// of failing the whole resolution. For integrations that exist on some
    /// nodes only (btcpay's internal-LND connection string: nodes without
    /// LND must still run btcpay, just without the internal node).
    #[serde(default)]
    pub optional: bool,
}

/// A fully resolved secret env entry, produced at apply time. `value` lives
/// only in memory on its way to the podman secret store (or, on the dev
/// docker fallback, straight into the container env).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretEnvRef {
    pub env_key: String,
    /// Podman secret object name: `archy-env-<app>-<KEY>`.
    pub secret_name: String,
    pub value: String,
}

/// Container label carrying the combined secret-env content hash, used by
/// the reconciler to detect secret rotation as env drift.
pub const SECRET_ENV_HASH_LABEL: &str = "io.archipelago.secret-env-hash";

/// Podman secret label carrying the individual secret's content hash, used
/// to skip re-registration when nothing changed.
pub const SECRET_HASH_LABEL: &str = "io.archipelago.hash";

/// Expand `${KEY}` placeholders across plain + secret values, then split
/// the result into (plain env, secret-bearing pairs). Any plain entry whose
/// value interpolates a secret key is *tainted* — it embeds the secret and
/// must travel as a secret itself (btcpay's
/// `BTCPAY_POSTGRES=…Password=${BTCPAY_DB_PASS};…` pattern). Secret values
/// themselves are taken verbatim: a generated secret that happens to
/// contain `${` must not be mangled by expansion.
pub fn expand_and_partition_env(
    plain: Vec<String>,
    secrets: Vec<(String, String)>,
) -> (Vec<String>, Vec<(String, String)>) {
    let plain_values: std::collections::HashMap<String, String> = plain
        .iter()
        .filter_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect();

    let mut out_plain = Vec::with_capacity(plain.len());
    let mut out_secret: Vec<(String, String)> = Vec::with_capacity(secrets.len());

    for entry in plain {
        let Some((key, value)) = entry.split_once('=') else {
            out_plain.push(entry);
            continue;
        };
        let mut expanded = value.to_string();
        let mut tainted = false;
        for (k, v) in &plain_values {
            expanded = expanded.replace(&format!("${{{k}}}"), v);
        }
        for (k, v) in &secrets {
            let placeholder = format!("${{{k}}}");
            if expanded.contains(&placeholder) {
                expanded = expanded.replace(&placeholder, v);
                tainted = true;
            }
        }
        if tainted {
            out_secret.push((key.to_string(), expanded));
        } else {
            out_plain.push(format!("{key}={expanded}"));
        }
    }

    out_secret.extend(secrets);
    (out_plain, out_secret)
}

/// How a [`GeneratedSecret`] is produced. Each kind is deterministic in shape
/// (so the orchestrator knows which files to expect) but random in value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretGenKind {
    /// 16 random bytes, lowercase hex (32 chars). Service passwords/API tokens.
    Hex16,
    /// 32 random bytes, lowercase hex (64 chars). Longer keys/cookies.
    Hex32,
    /// 32 random bytes, standard base64 (44 chars incl. padding). For services
    /// that require a base64-encoded key rather than hex — e.g. netbird's relay
    /// `authSecret` and the SQLite store `encryptionKey`, which base64-decode
    /// their configured value (hex would decode to the wrong bytes).
    Base64,
    /// A random password and its bcrypt hash. `<name>` holds the bcrypt hash
    /// (what a server is configured with); the plaintext is stored alongside as
    /// `<name>.pw` for any client that must authenticate. `secret_env` injects
    /// whichever file it references.
    Bcrypt,
}

/// A secret materialised by the orchestrator on demand. See
/// [`ContainerConfig::generated_secrets`]. `name` is a bare filename under the
/// secrets dir — validated (no `/`, no `..`) at [`AppManifest::validate`] time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedSecret {
    pub name: String,
    pub kind: SecretGenKind,
}

impl GeneratedSecret {
    /// Every file this secret materialises, in the order they should be written
    /// (primary first). A consumer references one of these via `secret_env`.
    pub fn target_files(&self) -> Vec<String> {
        match self.kind {
            SecretGenKind::Hex16 | SecretGenKind::Hex32 | SecretGenKind::Base64 => {
                vec![self.name.clone()]
            }
            SecretGenKind::Bcrypt => vec![self.name.clone(), format!("{}.pw", self.name)],
        }
    }
}

/// A self-signed TLS certificate materialised by the orchestrator. See
/// [`ContainerConfig::generated_certs`]. `crt`/`key` are absolute host paths
/// (typically under `/var/lib/archipelago/<app>/`) that the container
/// bind-mounts read-only. `common_name` and `sans` are rendered against host
/// facts (`{{HOST_IP}}`) at apply time; when omitted they default to the
/// node's host IP plus `IP:127.0.0.1,DNS:localhost` so the cert is valid for
/// however the box is reached locally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedCert {
    pub crt: String,
    pub key: String,
    #[serde(default)]
    pub common_name: Option<String>,
    #[serde(default)]
    pub sans: Vec<String>,
}

fn default_pull_policy() -> String {
    "if-not-present".to_string()
}

/// Build a container image locally from a Dockerfile rather than pulling from a registry.
///
/// When present on `ContainerConfig`, the orchestrator runs `podman build -t <tag> -f <dockerfile> <context>`
/// before starting the container. The resulting local image is referenced by `tag`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildConfig {
    /// Build context directory (absolute path or relative to the manifest location).
    pub context: String,
    /// Dockerfile path relative to `context`. Defaults to `Dockerfile`.
    #[serde(default = "default_dockerfile")]
    pub dockerfile: String,
    /// Tag applied to the built image. Used as the container's image reference.
    pub tag: String,
    /// Optional `--build-arg KEY=VALUE` pairs passed to the build.
    #[serde(default)]
    pub build_args: HashMap<String, String>,
}

fn default_dockerfile() -> String {
    "Dockerfile".to_string()
}

/// Resolved pull-or-build decision after manifest validation.
///
/// `ContainerConfig::resolve()` produces this. The orchestrator matches on it
/// to decide whether to pull a registry image or invoke a local build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSource {
    /// Pull `image` from a registry using `pull_policy` semantics.
    Pull {
        image: String,
        pull_policy: String,
        image_signature: Option<String>,
    },
    /// Build locally. The resulting tag is the image reference for `podman create`.
    Build(BuildConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Storage {
        storage: String,
    },
    App {
        app_id: String,
        version: Option<String>,
    },
    Simple(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    #[serde(default)]
    pub cpu_limit: Option<u32>,
    #[serde(default)]
    pub memory_limit: Option<String>,
    #[serde(default)]
    pub disk_limit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityPolicy {
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_true")]
    pub readonly_root: bool,
    #[serde(default = "default_true")]
    pub no_new_privileges: bool,
    #[serde(default = "default_network_policy")]
    pub network_policy: String,
    #[serde(default)]
    pub apparmor_profile: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_network_policy() -> String {
    "isolated".to_string()
}

/// Whether a published port must sit behind the node's app authentication
/// gate.
///
/// The default is deliberately the protected one. Every app port on this
/// node was reachable with no credential at all over LAN, Tailscale, Tor and
/// the FIPS mesh alike (reproduced 2026-08-03) precisely because exposure
/// was the thing you got by saying nothing. Making `Session` the default
/// inverts that: a new app is protected unless its manifest argues for an
/// exemption, and the exemptions are a `grep auth: none apps/` rather than a
/// discovery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortAuth {
    /// Default. The daemon's app gate authenticates every connection: a
    /// valid session (2FA honoured, since a session still pending its TOTP
    /// step fails validation) or an app-scoped bearer token for machine
    /// clients. Anything else gets the login page.
    #[default]
    Session,
    /// Exempt — the gate does not touch this port.
    ///
    /// Only legitimate when the port carries a protocol that authenticates
    /// itself (LND macaroons, Lightning's noise handshake, TLS client
    /// certs) or one where a login page would be meaningless and harmful
    /// (Bitcoin p2p gossip, mDNS). Requires `auth_rationale`: an exemption
    /// nobody can explain is an exemption nobody reviewed.
    None,
    /// Host-local by intent — the gate must not bind this port at all.
    ///
    /// This exists because `bind: 127.0.0.1` is ambiguous on its own, and
    /// reading intent out of it would be wrong in both directions. Two
    /// unrelated situations produce an identical loopback publish:
    ///
    /// * Bitcoin's RPC 8332 is loopback-pinned so that the LAN *cannot*
    ///   reach it. Fronting it with the gate would newly expose it on every
    ///   host address — behind a login, but exposed where it deliberately
    ///   was not.
    /// * A gated app is loopback-pinned precisely *so that* the gate can
    ///   take over its external addresses; that is the whole migration.
    ///
    /// Inferring from `bind` would break one or the other, so the intent is
    /// declared. `Local` means the first case: never externally reachable,
    /// gate keeps its hands off.
    Local,
    /// The app publishes on loopback ONLY, and the daemon owns this port's
    /// external addresses — bind them and authenticate every connection.
    ///
    /// This is the migrated end state, and it is opt-in for a reason. The
    /// gate binding an address is the one action that can make a port
    /// reachable where it previously was not, so it must never be something
    /// a manifest gets by default or by inference. An earlier revision
    /// gated any `session` port regardless of `bind`, which meant a node
    /// whose manifests had not yet been updated saw the daemon publish
    /// Bitcoin's loopback-only RPC on every host address (caught on
    /// a test node 2026-08-03, seconds after deploy). Requiring the
    /// manifest to say so means the loopback pin and the daemon takeover
    /// ship together, atomically, and a stale manifest fails safe.
    Gated,
    /// Like `gated` — loopback-pinned app, daemon owns the external
    /// addresses — but the gate does NOT require the dashboard login.
    ///
    /// For apps that carry a complete login of their own and are broken by
    /// an upstream challenge: Gitea (git clients speak basic-auth, not
    /// cookies), BTCPay (checkout pages must be reachable by anonymous
    /// payers). The gate still fronts the port — frame-header neutralising,
    /// the app-down retry page, the Tor upstream — it just lets every
    /// request through to the app's own authentication. Requires
    /// `auth_rationale`, exactly like `none`: an unchallenged surface nobody
    /// can explain is one nobody reviewed. The operator can flip any
    /// gate-fronted app between `gated` and `open` behaviour at runtime
    /// (Settings → app → App gate; stored node-side, manifest sets the
    /// default).
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
    #[serde(default)]
    pub protocol: String,
    /// Host address to bind the publish to. Empty = all interfaces
    /// (0.0.0.0). Set `127.0.0.1` to keep a port host-local; list the same
    /// host/container pair twice with different binds to serve several
    /// addresses (e.g. loopback + the archy-net gateway `10.89.0.1` so
    /// containers keep reaching it via `host.archipelago`).
    #[serde(default)]
    pub bind: String,
    /// Declared authentication policy, or `None` when the manifest says
    /// nothing at all.
    ///
    /// The distinction is load-bearing and was learned the hard way. A node's
    /// installed manifests always lag the binary, so "absent" is the state of
    /// essentially every port on every node until a signed catalog delivers
    /// otherwise. Treating absent as a *value* meant the daemon acted on a
    /// default the manifest never asked for: first republishing Bitcoin's
    /// loopback-only RPC across the LAN, then — caught before it shipped —
    /// preparing to pin LND's gRPC and REST to loopback, which would have
    /// broken Zeus and every remote wallet.
    ///
    /// So absent means "no instruction", and the daemon may only ever REPORT
    /// on such a port, never change how it is published. Use
    /// [`PortMapping::auth_policy`] for classification and
    /// [`PortMapping::auth_is_declared`] before acting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PortAuth>,
    /// Why this port is safe to expose unauthenticated. **Required** when
    /// `auth` is `none`, rejected otherwise — a rationale on a gated port
    /// means the author expected an exemption they did not get.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_rationale: Option<String>,
    /// Forward the node session cookie to the app on authorised requests.
    ///
    /// The gate normally strips its own credential before proxying — an app
    /// must never be in a position to log or replay the node session. The
    /// first-party companion UIs (lnd-ui, bitcoin-ui, electrs-ui, fips-ui)
    /// are the exception their design requires: their nginx forwards the
    /// browser's session cookie to the daemon's authenticated endpoints
    /// (`/proxy/lnd/*`, `/rpc/v1`, `/lnd-connect-info`), so stripping it
    /// breaks every data call behind the gate with a 401 while the page
    /// shell still renders (observed as "LND UI unreachable", 2026-08-05).
    /// Only meaningful on a `auth: gated` port.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub session_passthrough: bool,
}

impl PortMapping {
    /// Policy to classify this port by. An undeclared port reports as
    /// `Session` — i.e. it shows up in the audit as something that *should*
    /// be behind the gate — because reporting an unprotected port is always
    /// safe. Acting on it is not; see [`Self::auth_is_declared`].
    pub fn auth_policy(&self) -> PortAuth {
        self.auth.unwrap_or(PortAuth::Session)
    }

    /// Whether the manifest actually stated a policy. Required before the
    /// daemon rewrites how a port is published: silence is not consent.
    pub fn auth_is_declared(&self) -> bool {
        self.auth.is_some()
    }
}

impl From<(u16, u16)> for PortMapping {
    fn from((host, container): (u16, u16)) -> Self {
        PortMapping {
            host,
            container,
            protocol: "tcp".to_string(),
            bind: String::new(),
            auth: None,
            auth_rationale: None,
            session_passthrough: false,
        }
    }
}

/// True when the host can actually bind a publish to `ip` right now.
///
/// Rootless podman forwards published ports via rootlessport, which listens
/// in the HOST network namespace — an address that only exists inside the
/// rootless netns (e.g. the archy-net bridge gateway `10.89.0.1`) fails with
/// "cannot assign requested address" and systemd crash-loops the whole unit
/// (bitcoin went down fleet-wide-capable this way, 2026-07-09 on .228).
/// Callers drop such publishes instead of passing them through: a publish
/// that cannot bind provides nothing, while the failed bind takes the
/// container down with it.
///
/// Empty (= 0.0.0.0), wildcard, and loopback binds are always accepted
/// without probing. Anything else is probed with an ephemeral-port bind.
/// Unparseable addresses return false — podman would reject them anyway.
pub fn host_can_bind_publish_ip(ip: &str) -> bool {
    if ip.is_empty() {
        return true;
    }
    let Ok(addr) = ip.parse::<std::net::IpAddr>() else {
        return false;
    };
    if addr.is_unspecified() || addr.is_loopback() {
        return true;
    }
    std::net::TcpListener::bind((addr, 0)).is_ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    #[serde(rename = "type")]
    pub volume_type: String,
    #[serde(default)]
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub options: Vec<String>,
    /// For `type: tmpfs` only. Comma-separated mount options
    /// (e.g. `"rw,noexec,nosuid,size=256m"`). Ignored for bind/volume.
    #[serde(default)]
    pub tmpfs_options: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    #[serde(rename = "type")]
    pub check_type: String,
    pub endpoint: Option<String>,
    pub path: Option<String>,
    #[serde(default = "default_interval")]
    pub interval: String,
    #[serde(default = "default_timeout")]
    pub timeout: String,
    #[serde(default = "default_retries")]
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppInterface {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default = "default_interface_type")]
    pub interface_type: String,
    pub port: u16,
    #[serde(default = "default_http_protocol")]
    pub protocol: String,
    #[serde(default = "default_root_path")]
    pub path: String,
}

fn default_interface_type() -> String {
    "ui".to_string()
}

fn default_http_protocol() -> String {
    "http".to_string()
}

fn default_root_path() -> String {
    "/".to_string()
}

fn default_interval() -> String {
    "30s".to_string()
}

fn default_timeout() -> String {
    "5s".to_string()
}

fn default_retries() -> u32 {
    3
}

impl AppManifest {
    pub fn from_file(path: &std::path::Path) -> Result<Self, ManifestError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Result<Self, ManifestError> {
        let manifest: AppManifest = serde_yaml::from_str(content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if !is_valid_app_id(&self.app.id) {
            return Err(ManifestError::Invalid(
                "app.id must be lowercase ASCII letters, digits, or single hyphens".to_string(),
            ));
        }

        if self.app.name.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "app.name cannot be empty".to_string(),
            ));
        }

        // Exactly one of container.image or container.build must be set. We can't
        // default either side, because an empty-string image or an empty build block
        // would be silently wrong downstream.
        match (&self.app.container.image, &self.app.container.build) {
            (Some(img), None) if !img.is_empty() => {}
            (None, Some(b)) => {
                if b.context.is_empty() {
                    return Err(ManifestError::Invalid(
                        "container.build.context cannot be empty".to_string(),
                    ));
                }
                if b.tag.is_empty() {
                    return Err(ManifestError::Invalid(
                        "container.build.tag cannot be empty".to_string(),
                    ));
                }
            }
            (Some(_), Some(_)) => {
                return Err(ManifestError::Invalid(
                    "container.image and container.build are mutually exclusive".to_string(),
                ));
            }
            _ => {
                return Err(ManifestError::Invalid(
                    "container must specify either image or build".to_string(),
                ));
            }
        }

        // Validate version format (semantic versioning)
        if !self.app.version.chars().any(|c| c.is_ascii_digit()) {
            return Err(ManifestError::Invalid(
                "app.version must contain at least one digit".to_string(),
            ));
        }

        // ── Step 8b.0 field validation ────────────────────────────────

        // network: allow any non-empty string; podman itself is the
        // final authority (named networks, "host", "bridge", "none",
        // "container:<name>", etc). Reject only the empty-string case
        // so "network:" with no value is a loud error instead of a
        // silent default.
        if let Some(n) = &self.app.container.network {
            if n.is_empty() {
                return Err(ManifestError::Invalid(
                    "container.network cannot be empty (omit the field to use default)".to_string(),
                ));
            }
            if is_dangerous_network_mode(n) {
                return Err(ManifestError::Invalid(format!(
                    "container.network '{n}' is not allowed in app manifests"
                )));
            }
        }

        // network_aliases: each must be a non-empty DNS label (lowercase
        // alphanumeric + hyphen, no leading/trailing hyphen) so it renders as a
        // valid podman --network-alias / aardvark-dns name.
        for (i, alias) in self.app.container.network_aliases.iter().enumerate() {
            let ok = !alias.is_empty()
                && alias.len() <= 63
                && alias
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                && !alias.starts_with('-')
                && !alias.ends_with('-');
            if !ok {
                return Err(ManifestError::Invalid(format!(
                    "container.network_aliases[{i}] '{alias}' must be a non-empty DNS label \
                     (lowercase a-z, 0-9, '-'; no leading/trailing '-')"
                )));
            }
        }

        // custom_args: no empty strings (would inject literal "" into
        // the podman command line and confuse downstream parsing).
        for (i, a) in self.app.container.custom_args.iter().enumerate() {
            if a.is_empty() {
                return Err(ManifestError::Invalid(format!(
                    "container.custom_args[{i}] cannot be empty"
                )));
            }
        }

        // entrypoint: present ⇒ non-empty vec, no empty elements.
        if let Some(ep) = &self.app.container.entrypoint {
            if ep.is_empty() {
                return Err(ManifestError::Invalid(
                    "container.entrypoint must contain at least one element when set".to_string(),
                ));
            }
            for (i, a) in ep.iter().enumerate() {
                if a.is_empty() {
                    return Err(ManifestError::Invalid(format!(
                        "container.entrypoint[{i}] cannot be empty"
                    )));
                }
            }
        }

        // derived_env: non-empty keys, unique keys, templates reference
        // only known host-fact placeholders.
        {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for (i, e) in self.app.container.derived_env.iter().enumerate() {
                if e.key.is_empty() {
                    return Err(ManifestError::Invalid(format!(
                        "container.derived_env[{i}].key cannot be empty"
                    )));
                }
                if !seen.insert(e.key.as_str()) {
                    return Err(ManifestError::Invalid(format!(
                        "container.derived_env has duplicate key '{}'",
                        e.key
                    )));
                }
                validate_derived_template(&e.key, &e.template)?;
            }
        }

        // secret_env: non-empty keys, unique keys, secret_file is a
        // bare filename (no '/', no '..').
        {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for (i, e) in self.app.container.secret_env.iter().enumerate() {
                if e.key.is_empty() {
                    return Err(ManifestError::Invalid(format!(
                        "container.secret_env[{i}].key cannot be empty"
                    )));
                }
                if !seen.insert(e.key.as_str()) {
                    return Err(ManifestError::Invalid(format!(
                        "container.secret_env has duplicate key '{}'",
                        e.key
                    )));
                }
                if e.secret_file.is_empty()
                    || e.secret_file.contains('/')
                    || e.secret_file.contains("..")
                {
                    return Err(ManifestError::Invalid(format!(
                        "container.secret_env[{}].secret_file must be a bare filename (no '/', no '..'), got '{}'",
                        i, e.secret_file
                    )));
                }
            }
        }

        // generated_secrets: bare-filename names, unique across every file the
        // set materialises (so a Bcrypt's `.pw` sibling can't collide with
        // another secret). Path-safety mirrors secret_env.
        {
            let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
            for (i, g) in self.app.container.generated_secrets.iter().enumerate() {
                if g.name.is_empty() || g.name.contains('/') || g.name.contains("..") {
                    return Err(ManifestError::Invalid(format!(
                        "container.generated_secrets[{}].name must be a bare filename (no '/', no '..'), got '{}'",
                        i, g.name
                    )));
                }
                for f in g.target_files() {
                    if !names.insert(f.clone()) {
                        return Err(ManifestError::Invalid(format!(
                            "container.generated_secrets produces duplicate file '{f}'"
                        )));
                    }
                }
            }
        }

        // generated_certs: crt/key must be non-empty absolute paths with no
        // traversal (they become bind-mount sources, same safety bar as files).
        for (i, c) in self.app.container.generated_certs.iter().enumerate() {
            for (field, val) in [("crt", &c.crt), ("key", &c.key)] {
                if val.is_empty() || !val.starts_with('/') || val.contains("..") {
                    return Err(ManifestError::Invalid(format!(
                        "container.generated_certs[{i}].{field} must be an absolute path with no '..', got '{val}'"
                    )));
                }
            }
        }

        // data_uid: if set, must look like "NNNNN:NNNNN".
        if let Some(u) = &self.app.container.data_uid {
            let parts: Vec<&str> = u.split(':').collect();
            let valid = parts.len() == 2
                && !parts[0].is_empty()
                && !parts[1].is_empty()
                && parts[0].chars().all(|c| c.is_ascii_digit())
                && parts[1].chars().all(|c| c.is_ascii_digit());
            if !valid {
                return Err(ManifestError::Invalid(format!(
                    "container.data_uid must be 'UID:GID' with numeric parts, got '{}'",
                    u
                )));
            }
        }

        validate_security(&self.app.security)?;
        validate_ports(&self.app.ports)?;
        validate_interfaces(&self.app.interfaces)?;
        validate_environment(&self.app.environment)?;
        validate_devices(&self.app.devices)?;

        // Volume tmpfs_options: only meaningful for type: tmpfs.
        for (i, v) in self.app.volumes.iter().enumerate() {
            if v.volume_type == "tmpfs" {
                if v.target.is_empty() {
                    return Err(ManifestError::Invalid(format!(
                        "volumes[{i}] (tmpfs) must set target"
                    )));
                }
                if !v.source.is_empty() {
                    return Err(ManifestError::Invalid(format!(
                        "volumes[{i}] (tmpfs) must not set source"
                    )));
                }
            } else if v.tmpfs_options.is_some() {
                return Err(ManifestError::Invalid(format!(
                    "volumes[{i}] sets tmpfs_options but type is '{}', not 'tmpfs'",
                    v.volume_type
                )));
            } else {
                if v.volume_type != "bind" && v.volume_type != "volume" {
                    return Err(ManifestError::Invalid(format!(
                        "volumes[{i}].type must be bind, volume, or tmpfs"
                    )));
                }
                if v.source.is_empty() {
                    return Err(ManifestError::Invalid(format!(
                        "volumes[{i}] ({}) must set source",
                        v.volume_type
                    )));
                }
                if v.target.is_empty() {
                    return Err(ManifestError::Invalid(format!(
                        "volumes[{i}] ({}) must set target",
                        v.volume_type
                    )));
                }
                if v.volume_type == "bind" {
                    validate_bind_source(i, &v.source)?;
                } else if !is_valid_named_volume(&v.source) {
                    return Err(ManifestError::Invalid(format!(
                        "volumes[{i}].source must be a safe named volume"
                    )));
                }
                validate_container_path(i, &v.target)?;
                validate_volume_options(i, &v.options)?;
            }
        }

        for (i, f) in self.app.files.iter().enumerate() {
            if f.path.is_empty() {
                return Err(ManifestError::Invalid(format!(
                    "files[{i}].path cannot be empty"
                )));
            }
            if !std::path::Path::new(&f.path).is_absolute() {
                return Err(ManifestError::Invalid(format!(
                    "files[{i}].path must be absolute"
                )));
            }
            if f.content.is_empty() {
                return Err(ManifestError::Invalid(format!(
                    "files[{i}].content cannot be empty"
                )));
            }
            let file_path = std::path::Path::new(&f.path);
            let under_bind_mount = self
                .app
                .volumes
                .iter()
                .filter(|v| v.volume_type != "tmpfs" && !v.source.is_empty())
                .any(|v| file_path.starts_with(std::path::Path::new(&v.source)));
            if !under_bind_mount {
                return Err(ManifestError::Invalid(format!(
                    "files[{i}].path must live under a bind-mounted volume source"
                )));
            }
        }

        // Lifecycle hooks: declarative, allowlisted (no host exec, no absolute /
        // `..` copy sources). See docs/manifest-hooks-design.md.
        self.app.hooks.validate()?;

        Ok(())
    }
}

fn is_valid_app_id(id: &str) -> bool {
    if id.is_empty() || id.starts_with('-') || id.ends_with('-') || id.contains("--") {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_dangerous_network_mode(mode: &str) -> bool {
    mode.starts_with("container:") || mode.starts_with("ns:")
}

fn validate_security(policy: &SecurityPolicy) -> Result<(), ManifestError> {
    let allowed_network_policies = ["isolated", "bridge", "host"];
    if !policy.network_policy.is_empty()
        && !allowed_network_policies.contains(&policy.network_policy.as_str())
    {
        return Err(ManifestError::Invalid(format!(
            "security.network_policy must be one of {}",
            allowed_network_policies.join(", ")
        )));
    }

    let allowed_caps = [
        "CHOWN",
        "DAC_OVERRIDE",
        "FOWNER",
        "NET_ADMIN",
        "NET_BIND_SERVICE",
        "NET_RAW",
        "SETGID",
        "SETUID",
        "SYS_ADMIN",
    ];
    let mut seen = HashSet::new();
    for cap in &policy.capabilities {
        if !allowed_caps.contains(&cap.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "security.capabilities contains unsupported capability '{cap}'"
            )));
        }
        if !seen.insert(cap.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "security.capabilities contains duplicate capability '{cap}'"
            )));
        }
    }
    Ok(())
}

fn validate_ports(ports: &[PortMapping]) -> Result<(), ManifestError> {
    let mut seen_host = HashSet::new();
    for (i, port) in ports.iter().enumerate() {
        if port.host == 0 || port.container == 0 {
            return Err(ManifestError::Invalid(format!(
                "ports[{i}].host and ports[{i}].container must be non-zero"
            )));
        }
        let protocol = if port.protocol.is_empty() {
            "tcp"
        } else {
            port.protocol.as_str()
        };
        if protocol != "tcp" && protocol != "udp" {
            return Err(ManifestError::Invalid(format!(
                "ports[{i}].protocol must be tcp or udp"
            )));
        }
        if !port.bind.is_empty() && port.bind.parse::<std::net::IpAddr>().is_err() {
            return Err(ManifestError::Invalid(format!(
                "ports[{i}].bind must be an IP address, got '{}'",
                port.bind
            )));
        }
        // An exemption from the app gate has to carry its own justification.
        // Enforcing it here rather than at review time means the reason
        // exists in the manifest for every exempt port, so auditing the
        // node's unauthenticated surface is reading a list, not inferring
        // one from silence.
        match (port.auth_policy(), port.auth_rationale.as_ref()) {
            (PortAuth::None, None) => {
                return Err(ManifestError::Invalid(format!(
                    "ports[{i}] sets auth: none but no auth_rationale — an unauthenticated \
                     port must state why it is safe to expose"
                )));
            }
            (PortAuth::None, Some(rationale)) if rationale.trim().is_empty() => {
                return Err(ManifestError::Invalid(format!(
                    "ports[{i}].auth_rationale cannot be empty"
                )));
            }
            // `open` serves the app without the gate's login challenge, so it
            // carries the same burden of proof as `none`.
            (PortAuth::Open, None) => {
                return Err(ManifestError::Invalid(format!(
                    "ports[{i}] sets auth: open but no auth_rationale — a port served \
                     without the gate's login must state why (typically: the app \
                     enforces its own authentication)"
                )));
            }
            (PortAuth::Open, Some(rationale)) if rationale.trim().is_empty() => {
                return Err(ManifestError::Invalid(format!(
                    "ports[{i}].auth_rationale cannot be empty"
                )));
            }
            // A rationale on a gated port means the author wrote an
            // exemption and did not get one. Silently keeping the port
            // protected would be safe but misleading, so say so.
            (PortAuth::Session, Some(_)) => {
                return Err(ManifestError::Invalid(format!(
                    "ports[{i}] sets auth_rationale without auth: none — the port is gated \
                     and the rationale has no effect"
                )));
            }
            _ => {}
        }
        // The same host port may be listed more than once with different bind
        // addresses (e.g. loopback + the archy-net gateway); identical
        // (host, protocol, bind) triples are still rejected.
        if !seen_host.insert((port.host, protocol.to_string(), port.bind.clone())) {
            return Err(ManifestError::Invalid(format!(
                "ports contains duplicate host binding {}/{}",
                port.host, protocol
            )));
        }
    }
    Ok(())
}

fn validate_interfaces(interfaces: &HashMap<String, AppInterface>) -> Result<(), ManifestError> {
    let allowed_types = ["ui", "api", "metrics"];
    let allowed_protocols = ["http", "https"];
    for (key, interface) in interfaces {
        if !is_valid_interface_key(key) {
            return Err(ManifestError::Invalid(format!(
                "interfaces key '{key}' must be lowercase ASCII letters, digits, hyphens, or underscores"
            )));
        }
        if interface.port == 0 {
            return Err(ManifestError::Invalid(format!(
                "interfaces.{key}.port must be non-zero"
            )));
        }
        if !allowed_types.contains(&interface.interface_type.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "interfaces.{key}.type must be one of {}",
                allowed_types.join(", ")
            )));
        }
        if !allowed_protocols.contains(&interface.protocol.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "interfaces.{key}.protocol must be one of {}",
                allowed_protocols.join(", ")
            )));
        }
        if !interface.path.starts_with('/') || interface.path.chars().any(char::is_control) {
            return Err(ManifestError::Invalid(format!(
                "interfaces.{key}.path must start with '/' and contain no control characters"
            )));
        }
        if interface
            .name
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(ManifestError::Invalid(format!(
                "interfaces.{key}.name cannot be empty when set"
            )));
        }
        if interface
            .description
            .as_ref()
            .is_some_and(|description| description.trim().is_empty())
        {
            return Err(ManifestError::Invalid(format!(
                "interfaces.{key}.description cannot be empty when set"
            )));
        }
    }
    Ok(())
}

fn is_valid_interface_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn validate_environment(env: &[String]) -> Result<(), ManifestError> {
    let mut seen = HashSet::new();
    for (i, entry) in env.iter().enumerate() {
        let Some((key, _)) = entry.split_once('=') else {
            return Err(ManifestError::Invalid(format!(
                "environment[{i}] must be KEY=VALUE"
            )));
        };
        if !is_valid_env_key(key) {
            return Err(ManifestError::Invalid(format!(
                "environment[{i}] has invalid key '{key}'"
            )));
        }
        if !seen.insert(key) {
            return Err(ManifestError::Invalid(format!(
                "environment contains duplicate key '{key}'"
            )));
        }
    }
    Ok(())
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn validate_devices(devices: &[String]) -> Result<(), ManifestError> {
    let mut seen = HashSet::new();
    for (i, device) in devices.iter().enumerate() {
        if !device.starts_with("/dev/") || device.contains("..") {
            return Err(ManifestError::Invalid(format!(
                "devices[{i}] must be an absolute /dev path"
            )));
        }
        if !seen.insert(device.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "devices contains duplicate entry '{device}'"
            )));
        }
    }
    Ok(())
}

fn validate_bind_source(index: usize, source: &str) -> Result<(), ManifestError> {
    let path = std::path::Path::new(source);
    if !path.is_absolute() {
        if is_valid_named_volume(source) {
            return Ok(());
        }
        return Err(ManifestError::Invalid(format!(
            "volumes[{index}].source must be absolute for host bind mounts or a safe named volume"
        )));
    }
    if source.contains("..") {
        return Err(ManifestError::Invalid(format!(
            "volumes[{index}].source must not contain '..'"
        )));
    }
    if source.starts_with("/var/lib/archipelago/") || is_reviewed_host_bind_exception(source) {
        return Ok(());
    }
    Err(ManifestError::Invalid(format!(
        "volumes[{index}].source must be under /var/lib/archipelago or a reviewed host-bind exception"
    )))
}

fn is_reviewed_host_bind_exception(source: &str) -> bool {
    source == "/run/user/1000/podman/podman.sock" || source == "/var/run/dbus"
}

fn is_valid_named_volume(source: &str) -> bool {
    if source.is_empty() || source.contains('/') || source.contains("..") {
        return false;
    }
    source
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn validate_container_path(index: usize, target: &str) -> Result<(), ManifestError> {
    if !std::path::Path::new(target).is_absolute() || target.contains("..") {
        return Err(ManifestError::Invalid(format!(
            "volumes[{index}].target must be an absolute container path without '..'"
        )));
    }
    Ok(())
}

fn validate_volume_options(index: usize, options: &[String]) -> Result<(), ManifestError> {
    let allowed = ["rw", "ro", "z", "Z", "shared", "rshared", "slave", "rslave"];
    let mut seen = HashSet::new();
    for option in options {
        if !allowed.contains(&option.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "volumes[{index}].options contains unsupported option '{option}'"
            )));
        }
        if !seen.insert(option.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "volumes[{index}].options contains duplicate option '{option}'"
            )));
        }
    }
    Ok(())
}

/// Host facts available to `derived_env` templates at apply time.
///
/// Mirrors the values `scripts/container-specs.sh:detect_environment()`
/// computed before each reconcile pass. The Rust orchestrator computes
/// these once per reconcile tick and passes them to
/// `ContainerConfig::resolve_derived_env`.
#[derive(Debug, Clone)]
pub struct HostFacts {
    /// Primary host IPv4 (e.g. from `hostname -I | awk '{print $1}'`).
    /// Falls back to `127.0.0.1` on detection failure.
    pub host_ip: String,
    /// mDNS hostname (`<hostname>.local`). Survives DHCP churn and
    /// reinstall-on-different-IP. Requires avahi-daemon on the node.
    pub host_mdns: String,
    /// Usable disk size in gigabytes at `/var/lib/archipelago` (or
    /// `/` if the data partition is not yet mounted). Drives the
    /// prune-vs-full-node decision in bitcoin-knots custom_args.
    pub disk_gb: u64,
    /// Container name of the running Bitcoin node — `bitcoin-knots` or
    /// `bitcoin-core` — so dependents (mempool's CORE_RPC_HOST) reach the
    /// right host. Both are reachable on archy-net by their container name;
    /// only the name differs. Falls back to `bitcoin-knots` when undetected.
    pub bitcoin_host: String,
}

impl HostFacts {
    /// Test-only constant fixture; do not use in production paths.
    #[cfg(test)]
    pub fn sample() -> Self {
        Self {
            host_ip: "192.0.2.12".to_string(),
            host_mdns: "test-node.local".to_string(),
            disk_gb: 2000,
            bitcoin_host: "bitcoin-knots".to_string(),
        }
    }
}

/// Supported placeholder names in `DerivedEnv::template`. Keep in sync
/// with `HostFacts`. Centralized so validation and rendering agree.
const DERIVED_PLACEHOLDERS: &[&str] = &["HOST_IP", "HOST_MDNS", "DISK_GB", "BITCOIN_HOST"];

fn validate_derived_template(key: &str, template: &str) -> Result<(), ManifestError> {
    // Walk `{{NAME}}` occurrences and ensure each NAME is recognized.
    // Unbalanced braces are a user error.
    let bytes = template.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            let rest = &template[i + 2..];
            let close = rest.find("}}").ok_or_else(|| {
                ManifestError::Invalid(format!(
                    "container.derived_env['{key}'].template has unbalanced '{{{{' — no closing '}}}}'"
                ))
            })?;
            let name = &rest[..close];
            if !DERIVED_PLACEHOLDERS.contains(&name) {
                return Err(ManifestError::Invalid(format!(
                    "container.derived_env['{key}'].template references unknown placeholder '{{{{{name}}}}}' (supported: {})",
                    DERIVED_PLACEHOLDERS.join(", ")
                )));
            }
            i = i + 2 + close + 2;
        } else {
            i += 1;
        }
    }
    Ok(())
}

/// A source of named secrets. In prod this is a directory on disk
/// (`/var/lib/archipelago/secrets/`); in tests, a HashMap.
pub trait SecretsProvider {
    /// Read the named secret and return its value with trailing
    /// whitespace trimmed (so `echo "…" > secret-file` works without
    /// injecting a newline into env).
    fn read(&self, name: &str) -> Result<String, ManifestError>;
}

impl ContainerConfig {
    /// Collapse the (image, build) pair into a single resolved source.
    ///
    /// Returns `None` if the config is in an invalid state (e.g. neither field set
    /// or both set). Callers should have already run `AppManifest::validate()` to
    /// surface a user-facing error; this method is for internal orchestrator use
    /// after validation has passed.
    pub fn resolve(&self) -> Option<ResolvedSource> {
        match (&self.image, &self.build) {
            (Some(img), None) if !img.is_empty() => Some(ResolvedSource::Pull {
                image: img.clone(),
                pull_policy: self.pull_policy.clone(),
                image_signature: self.image_signature.clone(),
            }),
            (None, Some(b)) => Some(ResolvedSource::Build(b.clone())),
            _ => None,
        }
    }

    /// The image reference used to create/inspect a container for this config.
    ///
    /// For Pull sources this is the registry image. For Build sources this is
    /// the locally-built tag. Returns `None` only for an invalid config.
    pub fn image_ref(&self) -> Option<String> {
        self.resolve().map(|r| match r {
            ResolvedSource::Pull { image, .. } => image,
            ResolvedSource::Build(b) => b.tag,
        })
    }

    /// Render every `derived_env` entry's template against the given
    /// host facts. Returns `"KEY=VALUE"` strings ready to concatenate
    /// with `environment:`.
    ///
    /// Assumes `AppManifest::validate()` has already accepted the
    /// manifest — placeholder names are not re-checked here.
    pub fn resolve_derived_env(&self, facts: &HostFacts) -> Vec<String> {
        self.derived_env
            .iter()
            .map(|e| {
                let value = e
                    .template
                    .replace("{{HOST_IP}}", &facts.host_ip)
                    .replace("{{HOST_MDNS}}", &facts.host_mdns)
                    .replace("{{DISK_GB}}", &facts.disk_gb.to_string())
                    .replace("{{BITCOIN_HOST}}", &facts.bitcoin_host);
                format!("{}={}", e.key, value)
            })
            .collect()
    }

    /// Read every `secret_env` entry's value from the provider and
    /// return `"KEY=VALUE"` strings. Propagates the provider error on
    /// the first missing/unreadable secret — partial resolution is not
    /// useful because it silently produces a misconfigured container.
    pub fn resolve_secret_env(
        &self,
        provider: &dyn SecretsProvider,
    ) -> Result<Vec<String>, ManifestError> {
        Ok(self
            .resolve_secret_env_pairs(provider)?
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect())
    }

    /// Like `resolve_secret_env` but returns (key, value) pairs — the shape
    /// the podman-secret pipeline needs.
    pub fn resolve_secret_env_pairs(
        &self,
        provider: &dyn SecretsProvider,
    ) -> Result<Vec<(String, String)>, ManifestError> {
        let mut out = Vec::with_capacity(self.secret_env.len());
        for e in &self.secret_env {
            let v = match provider.read(&e.secret_file) {
                Ok(v) => v,
                Err(_) if e.optional => continue,
                Err(err) => return Err(err),
            };
            // An empty secret produces e.g. `-rpcpassword=` and crashes
            // the container on auth before logs are useful. Fail loud —
            // unless the entry is optional, where empty means "not set".
            if v.trim().is_empty() {
                if e.optional {
                    continue;
                }
                return Err(ManifestError::Invalid(format!(
                    "secret_env {} resolved to empty value (file: {})",
                    e.key, e.secret_file
                )));
            }
            out.push((e.key.clone(), v));
        }
        Ok(out)
    }
}

/// Deterministic content hash over resolved secret env pairs (sorted by
/// key), used for the container drift label and per-secret labels.
pub fn secret_env_content_hash(pairs: &[(String, String)]) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<&(String, String)> = pairs.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (k, v) in sorted {
        hasher.update(k.as_bytes());
        hasher.update([0u8]);
        hasher.update(v.as_bytes());
        hasher.update([0u8]);
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn host_can_bind_accepts_empty_wildcard_and_loopback_without_probing() {
        assert!(host_can_bind_publish_ip(""));
        assert!(host_can_bind_publish_ip("0.0.0.0"));
        assert!(host_can_bind_publish_ip("127.0.0.1"));
        assert!(host_can_bind_publish_ip("::"));
        assert!(host_can_bind_publish_ip("::1"));
    }

    #[test]
    fn host_can_bind_rejects_addresses_absent_from_the_host() {
        // TEST-NET-1 (RFC 5737) is never assigned to a real interface. The
        // production case is 10.89.0.1 (podman bridge gateway, exists only
        // inside the rootless netns), but that could legitimately bind on a
        // rootful host, so the test probes an address that can't.
        assert!(!host_can_bind_publish_ip("192.0.2.1"));
        assert!(!host_can_bind_publish_ip("not-an-ip"));
    }

    #[test]
    fn partition_taints_plain_entries_that_interpolate_secrets() {
        let plain = vec![
            "PLAIN=1".to_string(),
            "COMPOSED=${BASE}/x".to_string(),
            "DB_URL=postgres://u:${DB_PASS}@db/x".to_string(),
            "BASE=/srv".to_string(),
            "UNKNOWN=${NOT_DEFINED}".to_string(),
        ];
        let secrets = vec![("DB_PASS".to_string(), "s3cret".to_string())];
        let (p, s) = expand_and_partition_env(plain, secrets);

        // plain-from-plain expansion stays plain; unknown placeholders stay
        // literal (legacy expander parity)
        assert!(p.contains(&"PLAIN=1".to_string()));
        assert!(p.contains(&"COMPOSED=/srv/x".to_string()));
        assert!(p.contains(&"UNKNOWN=${NOT_DEFINED}".to_string()));
        // the tainted entry moved out of plain, fully expanded
        assert!(!p.iter().any(|e| e.starts_with("DB_URL=")));
        assert!(s.contains(&("DB_URL".to_string(), "postgres://u:s3cret@db/x".to_string())));
        // the original secret rides along verbatim
        assert!(s.contains(&("DB_PASS".to_string(), "s3cret".to_string())));
    }

    #[test]
    fn secret_values_are_never_expanded() {
        // A generated secret containing `${` must pass through untouched.
        let secrets = vec![("WEIRD".to_string(), "pa${PLAIN}ss".to_string())];
        let (_, s) = expand_and_partition_env(vec!["PLAIN=1".to_string()], secrets);
        assert!(s.contains(&("WEIRD".to_string(), "pa${PLAIN}ss".to_string())));
    }

    #[test]
    fn secret_env_hash_is_order_independent() {
        let a = vec![
            ("K1".to_string(), "v1".to_string()),
            ("K2".to_string(), "v2".to_string()),
        ];
        let b = vec![
            ("K2".to_string(), "v2".to_string()),
            ("K1".to_string(), "v1".to_string()),
        ];
        assert_eq!(secret_env_content_hash(&a), secret_env_content_hash(&b));
        let c = vec![("K1".to_string(), "CHANGED".to_string())];
        assert_ne!(secret_env_content_hash(&a), secret_env_content_hash(&c));
    }

    #[test]
    fn hooks_parse_and_validate() {
        let yaml = r#"
app:
  id: indeedhub
  name: IndeedHub
  version: 1.0.0
  container:
    image: test/indeedhub:1.0.0
  hooks:
    post_install:
      - exec: ["sed", "-i", "/X-Frame-Options/d", "/etc/nginx/conf.d/default.conf"]
      - copy_from_host:
          src: "web-ui/nostr-provider.js"
          dest: "/usr/share/nginx/html/nostr-provider.js"
"#;
        let m = AppManifest::parse(yaml).unwrap();
        assert_eq!(m.app.hooks.post_install.len(), 2);
        match &m.app.hooks.post_install[0] {
            HookStep::Exec { exec } => assert_eq!(exec[0], "sed"),
            _ => panic!("expected exec step"),
        }
        match &m.app.hooks.post_install[1] {
            HookStep::CopyFromHost { copy_from_host } => {
                assert_eq!(
                    copy_from_host.dest,
                    "/usr/share/nginx/html/nostr-provider.js"
                )
            }
            _ => panic!("expected copy_from_host step"),
        }
        m.validate().unwrap();
    }

    #[test]
    fn hooks_reject_absolute_or_traversal_copy_src() {
        for bad in ["/etc/passwd", "../../etc/shadow", "web-ui/../../etc/x"] {
            let yaml = format!(
                "app:\n  id: a\n  name: a\n  version: 1.0.0\n  container:\n    image: x:y\n  \
                 hooks:\n    post_install:\n      - copy_from_host:\n          src: \"{bad}\"\n          dest: \"/x\"\n"
            );
            assert!(
                AppManifest::parse(&yaml).is_err(),
                "src '{bad}' must be rejected"
            );
        }
    }

    /// Build a manifest with one port block, so each auth case differs only
    /// in the lines under test.
    fn manifest_with_port(port_yaml: &str) -> Result<AppManifest, ManifestError> {
        AppManifest::parse(&format!(
            "app:\n  id: a\n  name: a\n  version: 1.0.0\n  container:\n    image: x:y\n  ports:\n{port_yaml}"
        ))
    }

    /// Every manifest we ship must satisfy the schema — including the auth
    /// rules above. Without this the first exemption typo'd into a manifest
    /// would only surface when a node refused to load the app.
    #[test]
    fn all_shipped_manifests_parse() {
        let apps = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            return; // not a full checkout (vendored crate) — nothing to check
        };
        let mut checked = 0;
        for entry in entries.flatten() {
            let manifest = entry.path().join("manifest.yml");
            if !manifest.is_file() {
                continue;
            }
            let yaml = std::fs::read_to_string(&manifest).expect("manifest readable");
            AppManifest::parse(&yaml)
                .unwrap_or_else(|e| panic!("{} is invalid: {e}", manifest.display()));
            checked += 1;
        }
        assert!(checked > 40, "only found {checked} manifests — path wrong?");
    }

    /// The exempt set is the node's entire unauthenticated attack surface, so
    /// it must stay small and deliberate. If this count moves, someone added
    /// or removed an exemption and it wants a second pair of eyes.
    #[test]
    fn unauthenticated_ports_are_all_accounted_for() {
        let apps = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            return;
        };
        let mut exempt: Vec<(String, u16)> = Vec::new();
        for entry in entries.flatten() {
            let manifest = entry.path().join("manifest.yml");
            if !manifest.is_file() {
                continue;
            }
            let yaml = std::fs::read_to_string(&manifest).expect("manifest readable");
            let parsed = AppManifest::parse(&yaml).expect("manifest valid");
            for port in &parsed.app.ports {
                if port.auth_policy() == PortAuth::None {
                    exempt.push((parsed.app.id.clone(), port.host));
                }
            }
        }
        exempt.sort();
        // Reviewed 2026-09-30: lightning-stack's three retired endpoints
        // disappeared; Cuprate restricted RPC moved from none to gate-open.
        // Compare exact endpoints, not just a count that can hide substitutions.
        let expected = [
            ("bitcoin-core", 8333), ("bitcoin-knots", 8333),
            ("core-lightning", 9736), ("core-lightning", 9835),
            ("cuprate", 18183), ("electrumx", 50001),
            ("fedimint", 8173), ("fedimint", 8174),
            ("fedimint-gateway", 8176), ("fedimint-gateway", 9737),
            ("gitea", 2222), ("lnd", 9735), ("lnd", 10009), ("lnd", 18080),
            ("netbird", 8087), ("netbird-server", 3478), ("netbird-server", 8086),
            ("phoenixd", 9740), ("pine", 10381), ("pine-openwakeword", 10400),
            ("pine-piper", 10200), ("pine-whisper", 10300),
            ("router", 1900), ("router", 5353),
        ].into_iter().map(|(id, port)| (id.to_owned(), port)).collect::<Vec<_>>();
        assert_eq!(exempt, expected, "unauthenticated endpoint set changed; review each exemption");
    }

    /// `auth: open` ports are served by the gate WITHOUT its login challenge,
    /// so they are the second unauthenticated-by-the-gate surface and get the
    /// same review guard as `auth: none`. Each must enforce its own login or
    /// have an explicitly reviewed public protocol purpose.
    #[test]
    fn gate_open_ports_are_all_accounted_for() {
        let apps = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
        let Ok(entries) = std::fs::read_dir(&apps) else {
            return;
        };
        let mut open: Vec<(String, u16)> = Vec::new();
        for entry in entries.flatten() {
            let manifest = entry.path().join("manifest.yml");
            if !manifest.is_file() {
                continue;
            }
            let yaml = std::fs::read_to_string(&manifest).expect("manifest readable");
            let parsed = AppManifest::parse(&yaml).expect("manifest valid");
            for port in &parsed.app.ports {
                if port.auth_policy() == PortAuth::Open {
                    open.push((parsed.app.id.clone(), port.host));
                }
            }
        }
        open.sort();
        // Cuprate 18090 is its deliberately public restricted RPC subset;
        // unrestricted node-control RPC remains container-loopback-only.
        // Gitea 3001 (git clients speak basic-auth, not browser cookies),
        // BTCPay 23000 (checkout/invoice/webhook endpoints must be reachable
        // by anonymous payers), and — since the v1.8.7 platform round — the
        // three own-login consoles brought onto the manifest platform:
        // nginx-proxy-manager 8081 (NPM admin accounts), tailscale 8240
        // (tailnet login on the web console). Both enforce their own login,
        // and an operator can re-gate either from Settings → Access control.
        assert_eq!(
            open,
            vec![
                ("btcpay-server".to_string(), 23000u16),
                ("cuprate".to_string(), 18090u16),
                ("gitea".to_string(), 3001u16),
                ("nginx-proxy-manager".to_string(), 8081u16),
                ("tailscale".to_string(), 8240u16),
            ],
            "gate-open port set changed — review login or intentional public protocol purpose"
        );
    }

    #[test]
    fn an_undeclared_port_classifies_as_session_but_is_not_declared() {
        // Two different questions, and conflating them caused both gate
        // incidents. A manifest that says nothing must CLASSIFY as gated, so
        // the audit reports it as something that should be protected — but it
        // must not read as an instruction the daemon may act on.
        let manifest = manifest_with_port("    - host: 8080\n      container: 80\n").unwrap();
        let port = &manifest.app.ports[0];
        assert_eq!(port.auth_policy(), PortAuth::Session, "reports as gated");
        assert!(!port.auth_is_declared(), "but is NOT an instruction");
        assert!(port.auth.is_none());
    }

    #[test]
    fn an_explicit_session_declaration_is_actionable() {
        let manifest =
            manifest_with_port("    - host: 8080\n      container: 80\n      auth: session\n")
                .unwrap();
        let port = &manifest.app.ports[0];
        assert_eq!(port.auth_policy(), PortAuth::Session);
        assert!(port.auth_is_declared());
    }

    /// The wallet constraint, in the form that actually bit. LND's gRPC and
    /// REST carry `bind: ""`, so a rule keyed on `bind` alone does not save
    /// them — and on a node whose manifest predates the auth field there is
    /// no `auth: none` either. Undeclared must therefore be untouchable, or
    /// recreating LND silently pins those ports to loopback and every remote
    /// wallet stops working.
    #[test]
    fn an_undeclared_wallet_port_is_never_actionable() {
        let manifest =
            manifest_with_port("    - host: 10009\n      container: 10009\n      protocol: tcp\n")
                .unwrap();
        let port = &manifest.app.ports[0];
        assert!(port.bind.is_empty(), "this is the shape that bit us");
        assert!(
            !port.auth_is_declared(),
            "an undeclared port must never authorise republishing"
        );
    }

    #[test]
    fn port_auth_none_requires_a_rationale() {
        let err = manifest_with_port("    - host: 8333\n      container: 8333\n      auth: none\n")
            .expect_err("auth: none without a rationale must be rejected");
        assert!(
            err.to_string().contains("auth_rationale"),
            "error should name the missing field, got: {err}"
        );
    }

    #[test]
    fn port_auth_none_rejects_a_blank_rationale() {
        assert!(manifest_with_port(
            "    - host: 8333\n      container: 8333\n      auth: none\n      auth_rationale: \"   \"\n"
        )
        .is_err());
    }

    #[test]
    fn port_auth_none_with_a_rationale_parses() {
        let manifest = manifest_with_port(
            "    - host: 8333\n      container: 8333\n      auth: none\n      auth_rationale: Bitcoin p2p gossip\n",
        )
        .unwrap();
        assert_eq!(manifest.app.ports[0].auth, Some(PortAuth::None));
        assert_eq!(
            manifest.app.ports[0].auth_rationale.as_deref(),
            Some("Bitcoin p2p gossip")
        );
    }

    #[test]
    fn rationale_without_auth_none_is_rejected() {
        // Catches the author who wrote the justification but forgot the
        // `auth: none` line: the port stays gated, and shipping it silently
        // would leave them believing they had an exemption they never got.
        let err = manifest_with_port(
            "    - host: 8080\n      container: 80\n      auth_rationale: I meant to exempt this\n",
        )
        .expect_err("a rationale on a gated port must be rejected");
        assert!(err.to_string().contains("no effect"), "got: {err}");
    }

    #[test]
    fn hooks_reject_empty_exec() {
        let yaml = "app:\n  id: a\n  name: a\n  version: 1.0.0\n  container:\n    image: x:y\n  hooks:\n    post_install:\n      - exec: []\n";
        assert!(AppManifest::parse(yaml).is_err());
    }

    #[test]
    fn test_manifest_parse() {
        let yaml = r#"
app:
  id: test-app
  name: Test App
  version: 1.0.0
  container:
    image: test/image:latest
"#;

        let manifest = AppManifest::parse(yaml).unwrap();
        assert_eq!(manifest.app.id, "test-app");
        assert_eq!(manifest.app.name, "Test App");
        assert_eq!(manifest.app.version, "1.0.0");
    }

    #[test]
    fn typed_interfaces_parse_with_defaults() {
        let yaml = r#"
app:
  id: test-app
  name: Test App
  version: 1.0.0
  container:
    image: test/image:1.0.0
  interfaces:
    main:
      port: 8080
"#;

        let manifest = AppManifest::parse(yaml).unwrap();
        let main = manifest.app.interfaces.get("main").unwrap();
        assert_eq!(main.interface_type, "ui");
        assert_eq!(main.port, 8080);
        assert_eq!(main.protocol, "http");
        assert_eq!(main.path, "/");
    }

    #[test]
    fn invalid_interfaces_are_rejected() {
        let cases = [
            (
                "bad key",
                r#"
app:
  id: test-app
  name: Test App
  version: 1.0.0
  container:
    image: test/image:1.0.0
  interfaces:
    Bad Key:
      port: 8080
"#,
                "interfaces key",
            ),
            (
                "bad protocol",
                r#"
app:
  id: test-app
  name: Test App
  version: 1.0.0
  container:
    image: test/image:1.0.0
  interfaces:
    main:
      port: 8080
      protocol: ftp
"#,
                "interfaces.main.protocol",
            ),
            (
                "bad path",
                r#"
app:
  id: test-app
  name: Test App
  version: 1.0.0
  container:
    image: test/image:1.0.0
  interfaces:
    main:
      port: 8080
      path: dashboard
"#,
                "interfaces.main.path",
            ),
        ];

        for (name, yaml, expected) in cases {
            let err = AppManifest::parse(yaml).unwrap_err();
            let ManifestError::Invalid(msg) = err else {
                panic!("{name}: expected invalid manifest, got {err:?}");
            };
            assert!(
                msg.contains(expected),
                "{name}: expected error containing {expected:?}, got {msg:?}"
            );
        }
    }

    #[test]
    fn test_manifest_validation() {
        let yaml = r#"
app:
  id: ""
  name: Test
  version: 1.0.0
  container:
    image: test/image:latest
"#;

        let result = AppManifest::parse(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn pull_source_resolves_to_pull() {
        let yaml = r#"
app:
  id: test-app
  name: Test
  version: 1.0.0
  container:
    image: docker.io/library/nginx:1.27
    pull_policy: always
"#;
        let m = AppManifest::parse(yaml).unwrap();
        let src = m.app.container.resolve().unwrap();
        match src {
            ResolvedSource::Pull {
                image, pull_policy, ..
            } => {
                assert_eq!(image, "docker.io/library/nginx:1.27");
                assert_eq!(pull_policy, "always");
            }
            _ => panic!("expected Pull"),
        }
        assert_eq!(
            m.app.container.image_ref().as_deref(),
            Some("docker.io/library/nginx:1.27")
        );
    }

    #[test]
    fn build_source_resolves_to_build() {
        let yaml = r#"
app:
  id: bitcoin-ui
  name: Bitcoin UI
  version: 1.0.0
  container:
    build:
      context: /opt/archipelago/docker/bitcoin-ui
      dockerfile: Dockerfile
      tag: archy-bitcoin-ui:local
      build_args:
        NGINX_VERSION: "1.27"
"#;
        let m = AppManifest::parse(yaml).unwrap();
        let src = m.app.container.resolve().unwrap();
        match src {
            ResolvedSource::Build(b) => {
                assert_eq!(b.context, "/opt/archipelago/docker/bitcoin-ui");
                assert_eq!(b.dockerfile, "Dockerfile");
                assert_eq!(b.tag, "archy-bitcoin-ui:local");
                assert_eq!(b.build_args.get("NGINX_VERSION").unwrap(), "1.27");
            }
            _ => panic!("expected Build"),
        }
        assert_eq!(
            m.app.container.image_ref().as_deref(),
            Some("archy-bitcoin-ui:local")
        );
    }

    #[test]
    fn dockerfile_defaults_to_dockerfile() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    build:
      context: /tmp
      tag: x:local
"#;
        let m = AppManifest::parse(yaml).unwrap();
        match m.app.container.resolve().unwrap() {
            ResolvedSource::Build(b) => assert_eq!(b.dockerfile, "Dockerfile"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn image_and_build_both_set_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    image: foo:latest
    build:
      context: /tmp
      tag: x:local
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("mutually exclusive"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn neither_image_nor_build_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container: {}
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("either image or build"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn empty_image_string_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    image: ""
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("either image or build"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn empty_build_context_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    build:
      context: ""
      tag: x:local
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("context"), "unexpected error: {msg}");
    }

    #[test]
    fn empty_build_tag_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    build:
      context: /tmp
      tag: ""
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("tag"), "unexpected error: {msg}");
    }

    #[test]
    fn existing_pull_only_manifests_still_parse() {
        // Backwards-compat smoke: the shape every file in apps/*/manifest.yml uses today.
        let yaml = r#"
app:
  id: legacy
  name: Legacy App
  version: 0.1.0
  description: existing shape
  container:
    image: registry.example.com/legacy:1.2.3
    image_signature: sha256:abc
  ports:
    - { host: 8080, container: 80 }
"#;
        let m = AppManifest::parse(yaml).unwrap();
        assert_eq!(m.app.container.pull_policy, "if-not-present");
        matches!(
            m.app.container.resolve().unwrap(),
            ResolvedSource::Pull { .. }
        );
    }

    #[test]
    fn generated_files_must_live_under_bind_mounts() {
        let yaml = r#"
app:
  id: test-app
  name: Test App
  version: 1.0.0
  container:
    image: test/image:latest
  volumes:
    - type: bind
      source: /var/lib/archipelago/test-app
      target: /data
  files:
    - path: /var/lib/archipelago/test-app/config.yaml
      content: |
        key: value
"#;
        let manifest = AppManifest::parse(yaml).unwrap();
        assert_eq!(manifest.app.files.len(), 1);

        let bad = yaml.replace(
            "/var/lib/archipelago/test-app/config.yaml",
            "/etc/test-app/config.yaml",
        );
        let err = AppManifest::parse(&bad).unwrap_err();
        assert!(
            format!("{err}").contains("bind-mounted volume source"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn empty_custom_arg_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    image: foo:latest
    custom_args: [""]
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("custom_args[0]"), "unexpected error: {msg}");
    }

    #[test]
    fn empty_entrypoint_vec_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    image: foo:latest
    entrypoint: []
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("entrypoint"), "unexpected error: {msg}");
    }

    #[test]
    fn empty_entrypoint_element_is_rejected() {
        let yaml = r#"
app:
  id: x
  name: X
  version: 1.0.0
  container:
    image: foo:latest
    entrypoint: ["gatewayd", ""]
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("entrypoint[1]"), "unexpected error: {msg}");
    }

    #[test]
    fn duplicate_derived_env_keys_are_rejected() {
        let yaml = r#"
app:
  id: fedimint
  name: Fedimint
  version: 0.10.0
  container:
    image: fedimintd:v0.10.0
    derived_env:
      - key: FM_API_URL
        template: "ws://{{HOST_MDNS}}:8174"
      - key: FM_API_URL
        template: "ws://{{HOST_IP}}:8174"
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate key"), "unexpected error: {msg}");
    }

    #[test]
    fn unknown_derived_placeholder_is_rejected() {
        let yaml = r#"
app:
  id: fedimint
  name: Fedimint
  version: 0.10.0
  container:
    image: fedimintd:v0.10.0
    derived_env:
      - key: FM_API_URL
        template: "ws://{{HOSTNAME}}:8174"
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("unknown placeholder"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn path_traversal_secret_file_is_rejected() {
        let yaml = r#"
app:
  id: fedimint
  name: Fedimint
  version: 0.10.0
  container:
    image: fedimintd:v0.10.0
    secret_env:
      - key: FM_BITCOIND_PASSWORD
        secret_file: "../bitcoin-rpc-password"
"#;
        let err = AppManifest::parse(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("bare filename"), "unexpected error: {msg}");
    }

    #[test]
    fn resolve_derived_env_renders_host_facts() {
        let c = ContainerConfig {
            image: Some("x:latest".to_string()),
            image_signature: None,
            pull_policy: "if-not-present".to_string(),
            build: None,
            network: None,
            network_aliases: vec![],
            custom_args: vec![],
            entrypoint: None,
            derived_env: vec![
                DerivedEnv {
                    key: "FM_API_URL".to_string(),
                    template: "ws://{{HOST_MDNS}}:8174".to_string(),
                },
                DerivedEnv {
                    key: "INFO".to_string(),
                    template: "{{HOST_IP}}-{{DISK_GB}}".to_string(),
                },
                DerivedEnv {
                    key: "CORE_RPC_HOST".to_string(),
                    template: "{{BITCOIN_HOST}}".to_string(),
                },
            ],
            secret_env: vec![],
            generated_secrets: vec![],
            generated_certs: vec![],
            data_uid: None,
            secret_env_refs: vec![],
            secret_env_hash: None,
        };
        let facts = HostFacts {
            host_ip: "192.0.2.12".to_string(),
            host_mdns: "test-node.local".to_string(),
            disk_gb: 2000,
            bitcoin_host: "bitcoin-core".to_string(),
        };

        let out = c.resolve_derived_env(&facts);
        assert_eq!(out[0], "FM_API_URL=ws://test-node.local:8174");
        assert_eq!(out[1], "INFO=192.0.2.12-2000");
        assert_eq!(out[2], "CORE_RPC_HOST=bitcoin-core");
    }

    struct MapSecretsProvider {
        data: HashMap<String, String>,
    }

    impl SecretsProvider for MapSecretsProvider {
        fn read(&self, name: &str) -> Result<String, ManifestError> {
            self.data
                .get(name)
                .cloned()
                .ok_or_else(|| ManifestError::Invalid(format!("missing secret: {name}")))
        }
    }

    #[test]
    fn resolve_secret_env_reads_from_provider() {
        let c = ContainerConfig {
            image: Some("x:latest".to_string()),
            image_signature: None,
            pull_policy: "if-not-present".to_string(),
            build: None,
            network: None,
            network_aliases: vec![],
            custom_args: vec![],
            entrypoint: None,
            derived_env: vec![],
            secret_env: vec![
                SecretEnv {
                    key: "FM_BITCOIND_PASSWORD".to_string(),
                    secret_file: "bitcoin-rpc-password".to_string(),
                    optional: false,
                },
                SecretEnv {
                    key: "FM_GATEWAY_PASSWORD".to_string(),
                    secret_file: "fedimint-gateway-password".to_string(),
                    optional: false,
                },
            ],
            generated_secrets: vec![],
            generated_certs: vec![],
            data_uid: None,
            secret_env_refs: vec![],
            secret_env_hash: None,
        };
        let p = MapSecretsProvider {
            data: HashMap::from([
                (
                    "bitcoin-rpc-password".to_string(),
                    "supersecret1".to_string(),
                ),
                (
                    "fedimint-gateway-password".to_string(),
                    "supersecret2".to_string(),
                ),
            ]),
        };

        let out = c.resolve_secret_env(&p).unwrap();
        assert_eq!(out[0], "FM_BITCOIND_PASSWORD=supersecret1");
        assert_eq!(out[1], "FM_GATEWAY_PASSWORD=supersecret2");
    }

    #[test]
    fn resolve_secret_env_rejects_empty_value() {
        let c = ContainerConfig {
            image: Some("x:latest".to_string()),
            image_signature: None,
            pull_policy: "if-not-present".to_string(),
            build: None,
            network: None,
            network_aliases: vec![],
            custom_args: vec![],
            entrypoint: None,
            derived_env: vec![],
            secret_env: vec![SecretEnv {
                key: "BITCOIN_RPC_PASS".to_string(),
                secret_file: "bitcoin-rpc-password".to_string(),
                optional: false,
            }],
            generated_secrets: vec![],
            generated_certs: vec![],
            data_uid: None,
            secret_env_refs: vec![],
            secret_env_hash: None,
        };
        let p = MapSecretsProvider {
            data: HashMap::from([("bitcoin-rpc-password".to_string(), "   \n".to_string())]),
        };
        let err = c.resolve_secret_env(&p).unwrap_err();
        match err {
            ManifestError::Invalid(msg) => assert!(
                msg.contains("BITCOIN_RPC_PASS") && msg.contains("bitcoin-rpc-password"),
                "msg should name the env key + file: {msg}"
            ),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn resolve_secret_env_skips_missing_or_empty_optional_entries() {
        let c = ContainerConfig {
            image: Some("x:latest".to_string()),
            image_signature: None,
            pull_policy: "if-not-present".to_string(),
            build: None,
            network: None,
            network_aliases: vec![],
            custom_args: vec![],
            entrypoint: None,
            derived_env: vec![],
            secret_env: vec![
                SecretEnv {
                    key: "REQUIRED".to_string(),
                    secret_file: "present".to_string(),
                    optional: false,
                },
                SecretEnv {
                    key: "OPT_MISSING".to_string(),
                    secret_file: "does-not-exist".to_string(),
                    optional: true,
                },
                SecretEnv {
                    key: "OPT_EMPTY".to_string(),
                    secret_file: "empty".to_string(),
                    optional: true,
                },
            ],
            generated_secrets: vec![],
            generated_certs: vec![],
            data_uid: None,
            secret_env_refs: vec![],
            secret_env_hash: None,
        };
        let p = MapSecretsProvider {
            data: HashMap::from([
                ("present".to_string(), "value".to_string()),
                ("empty".to_string(), "  \n".to_string()),
            ]),
        };
        let out = c.resolve_secret_env(&p).unwrap();
        assert_eq!(out, vec!["REQUIRED=value".to_string()]);
    }

    #[test]
    fn unsafe_manifest_values_are_rejected() {
        let cases = [
            (
                "bad app id",
                r#"
app:
  id: Bad_App
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
"#,
                "app.id",
            ),
            (
                "unsupported capability",
                r#"
app:
  id: bad-cap
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  security:
    capabilities: [SYS_MODULE]
"#,
                "unsupported capability",
            ),
            (
                "docker socket bind",
                r#"
app:
  id: bad-bind
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  volumes:
    - type: bind
      source: /var/run/docker.sock
      target: /var/run/docker.sock
"#,
                "reviewed host-bind exception",
            ),
            (
                "path-like relative bind source",
                r#"
app:
  id: bad-bind
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  volumes:
    - type: bind
      source: data/cache
      target: /data
"#,
                "absolute for host bind mounts",
            ),
            (
                "bad environment key",
                r#"
app:
  id: bad-env
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  environment:
    - 1BAD=value
"#,
                "invalid key",
            ),
            (
                "duplicate host port",
                r#"
app:
  id: bad-port
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  ports:
    - { host: 8080, container: 80, protocol: tcp }
    - { host: 8080, container: 81, protocol: tcp }
"#,
                "duplicate host binding",
            ),
            (
                "duplicate host port with same bind",
                r#"
app:
  id: bad-port-bind
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  ports:
    - { host: 8332, container: 8332, protocol: tcp, bind: 127.0.0.1 }
    - { host: 8332, container: 8332, protocol: tcp, bind: 127.0.0.1 }
"#,
                "duplicate host binding",
            ),
            (
                "non-IP bind address",
                r#"
app:
  id: bad-bind
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  ports:
    - { host: 8332, container: 8332, protocol: tcp, bind: localhost }
"#,
                "bind must be an IP address",
            ),
            (
                "bad device",
                r#"
app:
  id: bad-device
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
  devices:
    - /tmp/fake-device
"#,
                "absolute /dev path",
            ),
            (
                "container network namespace",
                r#"
app:
  id: bad-network
  name: Bad
  version: 1.0.0
  container:
    image: test/image:latest
    network: container:host
"#,
                "not allowed",
            ),
        ];

        for (name, yaml, expected) in cases {
            let err = AppManifest::parse(yaml).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains(expected),
                "case {name} expected '{expected}', got: {msg}"
            );
        }
    }

    #[test]
    fn same_host_port_with_distinct_binds_is_valid() {
        // Bitcoin RPC hardening: 8332 published on loopback + the archy-net
        // gateway as two mappings of the same host port.
        let m = AppManifest::parse(
            r#"
app:
  id: bitcoin-knots
  name: Bitcoin Knots
  version: 1.0.0
  container:
    image: test/bitcoin:latest
  ports:
    - { host: 8332, container: 8332, protocol: tcp, bind: 127.0.0.1 }
    - { host: 8332, container: 8332, protocol: tcp, bind: 10.89.0.1 }
    - { host: 8333, container: 8333, protocol: tcp }
"#,
        )
        .expect("distinct binds on one host port must validate");
        assert_eq!(m.app.ports.len(), 3);
        assert_eq!(m.app.ports[0].bind, "127.0.0.1");
        assert_eq!(m.app.ports[1].bind, "10.89.0.1");
        assert_eq!(m.app.ports[2].bind, "");
    }

    #[test]
    fn reviewed_host_bind_exceptions_parse() {
        let yaml = r#"
app:
  id: reviewed-binds
  name: Reviewed Binds
  version: 1.0.0
  container:
    image: test/image:latest
  volumes:
    - type: bind
      source: /run/user/1000/podman/podman.sock
      target: /var/run/docker.sock
      options: [rw]
    - type: bind
      source: /var/run/dbus
      target: /var/run/dbus
      options: [ro]
"#;
        AppManifest::parse(yaml).unwrap();
    }

    #[test]
    fn parse_every_real_manifest() {
        let app_manifests = list_repo_manifests();
        assert!(
            !app_manifests.is_empty(),
            "no apps/*/manifest.yml files found"
        );

        let mut failures: Vec<String> = Vec::new();
        let mut modern_count = 0usize;
        for path in app_manifests {
            let content = fs::read_to_string(&path).expect("read manifest");
            let parsed_yaml: serde_yaml::Value = match serde_yaml::from_str(&content) {
                Ok(v) => v,
                Err(err) => {
                    failures.push(format!("{}: YAML parse error: {err}", path.display()));
                    continue;
                }
            };

            let is_modern = parsed_yaml
                .as_mapping()
                .map(|m| m.contains_key(serde_yaml::Value::String("app".to_string())))
                .unwrap_or(false);

            if is_modern {
                modern_count += 1;
                if let Err(err) = AppManifest::parse(&content) {
                    failures.push(format!("{}: {err}", path.display()));
                }
            } else {
                failures.push(format!(
                    "{}: expected modern app-schema manifest",
                    path.display()
                ));
            }
        }

        assert!(modern_count > 0, "no modern app-schema manifests found");

        assert!(
            failures.is_empty(),
            "manifest parse failures:\n{}",
            failures.join("\n")
        );
    }

    fn list_repo_manifests() -> Vec<PathBuf> {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let apps_dir = repo_root.join("apps");
        let mut out = Vec::new();

        let Ok(entries) = fs::read_dir(apps_dir) else {
            return out;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest = path.join("manifest.yml");
            if manifest.exists() {
                out.push(manifest);
            }
        }

        out.sort();
        out
    }
}
