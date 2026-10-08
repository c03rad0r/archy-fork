//! Private enrollment for the optional manifest-owned public-web router.
//! Secrets never enter website state, status responses, or generated content.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

static LOCK: Mutex<()> = Mutex::const_new(());
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
    pub host: String,
    pub port: u16,
    pub node_id: String,
    pub transport_token: String,
    pub enrollment_token: String,
    pub ca_pem: String,
    pub tls_server_name: String,
    pub domains: Vec<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: u32,
    gateway: Enrollment,
    certificate_mode: String,
    routes: Vec<WebsiteRoute>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WebsiteRoute {
    #[serde(default)]
    app_id: Option<String>,
    id: String,
    domain: String,
    fips_address: String,
    port: u16,
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && value.as_bytes()[0] != b'-'
}
impl Enrollment {
    fn validate(&self) -> Result<()> {
        for host in [&self.host, &self.tls_server_name] {
            if host.parse::<std::net::IpAddr>().is_err() {
                anyhow::ensure!(
                    super::hostname(host)? == *host,
                    "Use a lowercase gateway hostname"
                );
            }
        }
        anyhow::ensure!(
            self.port >= 1024 && name(&self.node_id),
            "Invalid gateway port or node enrollment name"
        );
        for token in [&self.transport_token, &self.enrollment_token] {
            anyhow::ensure!(
                (32..=256).contains(&token.len()) && !token.chars().any(char::is_control),
                "Invalid gateway credential"
            );
        }
        anyhow::ensure!(
            self.ca_pem.len() <= 16384
                && self.ca_pem.starts_with("-----BEGIN CERTIFICATE-----")
                && !self.ca_pem.contains("PRIVATE KEY"),
            "Supply the gateway CA certificate, never a private key"
        );
        reqwest::Certificate::from_pem(self.ca_pem.as_bytes())
            .context("Invalid gateway CA certificate")?;
        anyhow::ensure!(
            !self.domains.is_empty() && self.domains.len() <= 32,
            "Gateway enrollment needs assigned domains"
        );
        for domain in &self.domains {
            anyhow::ensure!(
                super::hostname(domain)? == *domain,
                "Use lowercase assigned domains"
            );
        }
        Ok(())
    }
}
async fn load(root: &Path) -> Result<Option<Config>> {
    let path = root.join("public-web-router/config/router.json");
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            anyhow::ensure!(bytes.len() <= 131072, "Gateway configuration exceeds limit");
            Ok(Some(
                serde_json::from_slice(&bytes).context("Invalid private gateway configuration")?,
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
async fn store(root: &Path, config: &Config) -> Result<()> {
    anyhow::ensure!(
        config.routes.len() <= 32,
        "Gateway supports at most 32 routes"
    );
    let dir = root.join("public-web-router/config");
    tokio::fs::create_dir_all(&dir).await?;
    let bytes = serde_json::to_vec(config)?;
    anyhow::ensure!(bytes.len() <= 131072, "Gateway configuration exceeds limit");
    let stage = dir.join(format!(".router-{}", uuid::Uuid::new_v4()));
    let mut opts = tokio::fs::OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts.open(&stage).await?;
    file.write_all(&bytes).await?;
    file.sync_all().await?;
    tokio::fs::rename(&stage, dir.join("router.json")).await?;
    tokio::fs::File::open(&dir).await?.sync_all().await?;
    Ok(())
}
fn public_status(config: Option<&Config>) -> Value {
    match config {
        None => json!({"configured":false,"routes":[],"externally_verified":false}),
        Some(c) => {
            json!({"configured":true,"host":c.gateway.host,"port":c.gateway.port,"domains":c.gateway.domains,"certificate_mode":c.certificate_mode,"routes":c.routes.iter().map(|r| json!({"id":r.id,"domain":r.domain})).collect::<Vec<_>>(),"externally_verified":false})
        }
    }
}
pub async fn status(root: &Path) -> Result<Value> {
    Ok(public_status(load(root).await?.as_ref()))
}
pub async fn configure(root: &Path, enrollment: Enrollment, mode: String) -> Result<Value> {
    let _guard = LOCK.lock().await;
    enrollment.validate()?;
    anyhow::ensure!(
        matches!(mode.as_str(), "public" | "test"),
        "Choose public or test certificates"
    );
    // A changed enrollment never silently sends existing sites to a new gateway.
    let config = Config {
        schema: 1,
        gateway: enrollment,
        certificate_mode: mode,
        routes: vec![],
    };
    store(root, &config).await?;
    Ok(public_status(Some(&config)))
}
pub async fn route(
    root: &Path,
    id: &str,
    enabled: bool,
    fips: Option<std::net::Ipv6Addr>,
) -> Result<Value> {
    let _guard = LOCK.lock().await;
    let mut config = load(root).await?.context("Connect your gateway first")?;
    if enabled {
        let state = super::load(root).await?;
        let project = state
            .projects
            .get(id)
            .context("Website project not found")?;
        anyhow::ensure!(
            project.routes.contains(&super::Route::PublicWeb),
            "Select public web and save this website first"
        );
        let domain = project
            .domain
            .as_ref()
            .context("Save this website's domain first")?
            .hostname
            .clone();
        anyhow::ensure!(
            config.gateway.domains.contains(&domain),
            "This domain is not assigned by your gateway enrollment"
        );
        let publication = project
            .fips_publication
            .as_ref()
            .context("Publish the website upstream first")?;
        anyhow::ensure!(
            (32000..32032).contains(&publication.port),
            "Invalid website listener"
        );
        let address = fips.context("FIPS is unavailable; start the node connection first")?;
        anyhow::ensure!(address.octets()[0] == 0xfd, "FIPS must use a ULA address");
        if config
            .routes
            .iter()
            .any(|r| r.domain == domain && r.id != id)
        {
            bail!("This domain already routes another website");
        }
        config.routes.retain(|r| r.id != id);
        config.routes.push(WebsiteRoute {
            app_id: None,
            id: id.to_owned(),
            domain,
            fips_address: address.to_string(),
            port: publication.port,
        });
    } else {
        config.routes.retain(|r| r.id != id);
    }
    store(root, &config).await?;
    Ok(public_status(Some(&config)))
}
/// Caller resolves the port from the live, guest-enabled catalogue app gate.
pub async fn app_route(
    root: &Path,
    app_id: &str,
    domain: &str,
    enabled: bool,
    address: Option<std::net::Ipv6Addr>,
    port: Option<u16>,
) -> Result<Value> {
    let _guard = LOCK.lock().await;
    anyhow::ensure!(name(app_id), "Invalid app identity");
    let mut config = load(root).await?.context("Connect your gateway first")?;
    let id = format!("app-{app_id}");
    anyhow::ensure!(name(&id), "App identity is too long for a gateway route");
    if enabled {
        let domain = super::hostname(domain)?;
        anyhow::ensure!(
            config.gateway.domains.contains(&domain),
            "This domain is not assigned by your gateway enrollment"
        );
        anyhow::ensure!(
            !config
                .routes
                .iter()
                .any(|r| r.domain == domain && r.id != id),
            "This domain already routes another service"
        );
        let address = address.context("FIPS is unavailable")?;
        anyhow::ensure!(address.octets()[0] == 0xfd, "FIPS must use a ULA address");
        let port = port.context("This app does not currently allow guest sharing")?;
        anyhow::ensure!(port >= 1024, "Invalid gated app port");
        config.routes.retain(|r| r.id != id);
        config.routes.push(WebsiteRoute {
            id,
            app_id: Some(app_id.to_owned()),
            domain,
            fips_address: address.to_string(),
            port,
        });
    } else {
        config.routes.retain(|r| r.id != id);
    }
    anyhow::ensure!(
        config.routes.len() <= 32,
        "Gateway supports at most 32 routes"
    );
    store(root, &config).await?;
    Ok(public_status(Some(&config)))
}
pub async fn disconnect(root: &Path) -> Result<Value> {
    let _guard = LOCK.lock().await;
    let path = root.join("public-web-router/config/router.json");
    match tokio::fs::remove_file(path).await {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    Ok(public_status(None))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            schema: 1,
            gateway: Enrollment {
                host: "gateway.example".into(),
                port: 7400,
                node_id: "node-a".into(),
                transport_token: "secret-transport-value".repeat(3),
                enrollment_token: "secret-enrollment-value".repeat(3),
                ca_pem: "test-certificate".into(),
                tls_server_name: "gateway.example".into(),
                domains: vec!["site.example".into()],
            },
            certificate_mode: "test".into(),
            routes: vec![],
        }
    }
    #[tokio::test]
    async fn private_enrollment_is_never_returned_and_disconnect_preserves_certificates() {
        let dir = tempfile::tempdir().unwrap();
        let c = config();
        store(dir.path(), &c).await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                tokio::fs::metadata(dir.path().join("public-web-router/config/router.json"))
                    .await
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let status = status(dir.path()).await.unwrap().to_string();
        assert!(!status.contains("secret"));
        assert!(!status.contains("test-certificate"));
        assert!(status.contains("gateway.example"));
        let data = dir.path().join("public-web-router/data");
        tokio::fs::create_dir_all(&data).await.unwrap();
        tokio::fs::write(data.join("certificate-marker"), b"preserve")
            .await
            .unwrap();
        disconnect(dir.path()).await.unwrap();
        assert!(load(dir.path()).await.unwrap().is_none());
        assert_eq!(
            tokio::fs::read(data.join("certificate-marker"))
                .await
                .unwrap(),
            b"preserve"
        );
    }
    #[tokio::test]
    async fn refuses_routing_unsaved_projects_and_never_accepts_raw_targets() {
        let dir = tempfile::tempdir().unwrap();
        store(dir.path(), &config()).await.unwrap();
        assert!(route(
            dir.path(),
            "missing",
            true,
            Some("fd00::1".parse().unwrap())
        )
        .await
        .is_err());
        assert!(load(dir.path()).await.unwrap().unwrap().routes.is_empty());
    }
    #[tokio::test]
    async fn app_routes_require_resolved_guest_port_and_assigned_domain() {
        let dir = tempfile::tempdir().unwrap();
        store(dir.path(), &config()).await.unwrap();
        let address = Some("fd00::1".parse().unwrap());
        assert!(app_route(
            dir.path(),
            "photoprism",
            "site.example",
            true,
            address,
            None
        )
        .await
        .is_err());
        assert!(app_route(
            dir.path(),
            "photoprism",
            "unassigned.example",
            true,
            address,
            Some(2342)
        )
        .await
        .is_err());
        app_route(
            dir.path(),
            "photoprism",
            "site.example",
            true,
            address,
            Some(2342),
        )
        .await
        .unwrap();
        assert_eq!(
            load(dir.path()).await.unwrap().unwrap().routes[0]
                .app_id
                .as_deref(),
            Some("photoprism")
        );
        app_route(dir.path(), "photoprism", "", false, None, None)
            .await
            .unwrap();
        assert!(load(dir.path()).await.unwrap().unwrap().routes.is_empty());
    }

    #[test]
    fn enrollment_rejects_invalid_certificates_and_names() {
        let mut c = config();
        assert!(c.gateway.validate().is_err());
        c.gateway.node_id = "../another-node".into();
        assert!(c.gateway.validate().is_err());
        assert!(!name(""));
        assert!(!name("-node"));
        assert!(name("node-a"));
    }
}
