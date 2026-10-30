//! Local client for the official `tollgate-installer` headless API.
//!
//! The installer is the single install/flash engine. Archy runs it on the
//! loopback interface (a node service, not a container), drives the same API
//! the official launcher uses, and then applies its own config/SSID/status
//! steps. The installer's UI is never exposed.
//!
//! The process is started on first use and re-started if it has exited. Its
//! state (SSH known-hosts, the recovery credential file) lives under
//! `<data_dir>/tollgate-installer`, so a backend restart does not lose the
//! trust store.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

const DEFAULT_PORT: u16 = 8099;
const DEFAULT_BIN: &str = "/usr/local/bin/tollgate-installer";
const LOG_NAME: &str = "installer.log";

/// A thin HTTP client for the installer's loopback API.
pub struct InstallerClient {
    base: String,
    state_dir: PathBuf,
    http: reqwest::Client,
}

impl InstallerClient {
    pub fn new(data_dir: &Path) -> Self {
        let port = std::env::var("ARCHY_TOLLGATE_INSTALLER_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base: format!("http://127.0.0.1:{port}"),
            state_dir: data_dir.join("tollgate-installer"),
            http,
        }
    }

    /// Ensure the local installer answers `/api/config`, starting it if
    /// necessary. Returns the config payload (version/feed tag).
    pub async fn ensure_running(&self) -> Result<Value> {
        if let Ok(cfg) = self.config().await {
            return Ok(cfg);
        }

        // A listener that is not our installer is worse than no listener: do
        // not spawn a second process on top of it.
        if tokio::net::TcpStream::connect(("127.0.0.1", self.port()))
            .await
            .is_ok()
        {
            anyhow::bail!(
                "port {} is already in use but does not answer the tollgate-installer API",
                self.port()
            );
        }

        self.spawn().await?;
        let mut last = None;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            match self.config().await {
                Ok(cfg) => return Ok(cfg),
                Err(err) => last = Some(err),
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("tollgate-installer did not start")))
            .context("start the local tollgate-installer")
    }

    async fn spawn(&self) -> Result<()> {
        let bin = std::env::var("ARCHY_TOLLGATE_INSTALLER_BIN")
            .unwrap_or_else(|_| DEFAULT_BIN.to_string());
        if !Path::new(&bin).is_file() {
            anyhow::bail!(
                "tollgate-installer binary not found at {bin} — install it, or set \
                 ARCHY_TOLLGATE_INSTALLER_BIN"
            );
        }
        std::fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("create {}", self.state_dir.display()))?;

        let log_path = self.state_dir.join(LOG_NAME);
        let out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("open {}", log_path.display()))?;
        let err = out.try_clone().context("clone installer log handle")?;

        let mut cmd = tokio::process::Command::new(&bin);
        cmd.args(["--bind", "127.0.0.1", "--port", &self.port().to_string()])
            .env("HOME", &self.state_dir)
            .env(
                "TOLLGATE_CREDENTIAL_FILE",
                self.state_dir.join("root-credentials"),
            )
            // File/env-driven Archy profile. The released installer ignores
            // unknown envs; the Archy fork reads them. The post-config below
            // is still the final writer, so this is a head start, not a
            // dependency.
            .env("TOLLGATE_BRAND", "archipelago")
            .env("TOLLGATE_PRIVATE_NYM", "noderunner-private")
            .env("TOLLGATE_SSID_PREFIXES_FILE", "/etc/tollgate/ssid-prefixes")
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .stdin(Stdio::null());
        cmd.spawn()
            .with_context(|| format!("spawn {bin} on 127.0.0.1:{}", self.port()))?;
        Ok(())
    }

    fn port(&self) -> u16 {
        self.base
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT)
    }

    pub async fn config(&self) -> Result<Value> {
        self.get("/api/config").await
    }

    pub async fn scan(&self) -> Result<Value> {
        self.get("/api/scan").await
    }

    pub async fn identify(&self, ip: &str, password: &str) -> Result<Value> {
        self.post("/api/identify", json!({ "ip": ip, "password": password }))
            .await
    }

    pub async fn trust(&self, ip: &str, fingerprint: &str) -> Result<Value> {
        self.post(
            "/api/trust-host-key",
            json!({ "ip": ip, "fingerprint": fingerprint }),
        )
        .await
    }

    /// Start a deploy. Returns the installer's job id.
    pub async fn deploy(&self, body: Value) -> Result<String> {
        let value = self.post("/api/deploy", body).await?;
        value
            .get("job_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("tollgate-installer returned no job_id"))
    }

    pub async fn job(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/status/{id}")).await
    }

    /// Poll a deploy to completion. Returns the final snapshot, whose `ip` is
    /// the management address after any post-install relocation.
    pub async fn wait(&self, id: &str, timeout: Duration) -> Result<Value> {
        let started = std::time::Instant::now();
        loop {
            let snapshot = self.job(id).await?;
            let status = snapshot
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("running");
            match status {
                // `pending` is the installer's non-fatal terminal state: the
                // package/portal installed, but the merchant wallet is still
                // loading. Archy's post-config/restart follows, so treat it as
                // success rather than timing out.
                "done" | "pending" => return Ok(snapshot),
                "failed" => {
                    let err = snapshot
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tollgate-installer deploy failed");
                    anyhow::bail!("tollgate-installer deploy failed: {err}");
                }
                _ => {}
            }
            if started.elapsed() > timeout {
                anyhow::bail!(
                    "tollgate-installer deploy timed out after {}s (job {id})",
                    timeout.as_secs()
                );
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let resp = self
            .http
            .get(format!("{}{}", self.base, path))
            .send()
            .await
            .with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("tollgate-installer GET {path} → {status}: {body}");
        }
        serde_json::from_str(&body).with_context(|| format!("parse {path} response"))
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let resp = self
            .http
            .post(format!("{}{}", self.base, path))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {path}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("tollgate-installer POST {path} → {status}: {text}");
        }
        serde_json::from_str(&text).with_context(|| format!("parse {path} response"))
    }
}
