//! Website-only Tor process. It never reloads the system Tor daemon, rewrites
//! application onions or deletes identity keys. Unpublish closes only that site's
//! HTTP listener before reloading this process's owned configuration.
use super::{serving, Route, State};
use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::LazyLock,
};
use tokio::{
    process::{Child, Command},
    sync::{watch, RwLock},
    task::JoinSet,
};

#[derive(Clone, serde::Serialize)]
pub struct TorStatus {
    pub project_id: String,
    pub onion_address: Option<String>,
    pub listening: bool,
    pub externally_verified: bool,
    pub error: Option<String>,
}
static STATUS: LazyLock<RwLock<Vec<TorStatus>>> = LazyLock::new(|| RwLock::new(vec![]));
pub async fn status() -> Vec<TorStatus> {
    STATUS.read().await.clone()
}

fn quoted(path: &Path) -> Result<String> {
    let s = path.to_str().context("Tor requires a UTF-8 data path")?;
    if !path.is_absolute() || s.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        bail!("Unsupported Tor website data path");
    }
    Ok(format!("\"{s}\""))
}
fn render(root: &Path, sites: &BTreeMap<String, u16>) -> Result<String> {
    let base = root.join("publishing/onions");
    let mut text = format!("# Owned by Archipelago website publishing\nDataDirectory {}\nSocksPort 0\nControlPort 0\nRunAsDaemon 0\nLog notice stdout\n", quoted(&base.join("runtime"))?);
    for (id, port) in sites {
        if uuid::Uuid::parse_str(id)
            .map(|u| u.to_string() != *id)
            .unwrap_or(true)
            || !(32100..32132).contains(port)
        {
            bail!("Invalid onion website identity or port");
        }
        text.push_str(&format!(
            "HiddenServiceDir {}\nHiddenServiceVersion 3\nHiddenServicePort 80 127.0.0.1:{port}\n",
            quoted(&base.join(id))?
        ));
    }
    Ok(text)
}
async fn private_dir(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}
async fn configure(
    root: &Path,
    sites: &BTreeMap<String, u16>,
    child: &mut Option<Child>,
    previous: &mut String,
) -> Result<()> {
    if let Some(process) = child.as_mut() {
        if process.try_wait()?.is_some() {
            *child = None;
            previous.clear();
        }
    }
    if sites.is_empty() {
        if let Some(mut process) = child.take() {
            process.kill().await?;
        }
        previous.clear();
        return Ok(());
    }
    let next = render(root, sites)?;
    if *previous == next && child.is_some() {
        return Ok(());
    }
    let base = root.join("publishing/onions");
    private_dir(&base).await?;
    private_dir(&base.join("runtime")).await?;
    for id in sites.keys() {
        private_dir(&base.join(id)).await?;
    }
    let stage = base.join("torrc.next");
    let config = base.join("torrc");
    tokio::fs::write(&stage, &next).await?;
    let checked = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        Command::new("tor")
            .args(["--defaults-torrc", "/dev/null", "-f"])
            .arg(&stage)
            .arg("--verify-config")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("Website Tor validation timed out")??;
    if !checked.status.success() {
        bail!("Website Tor rejected its configuration; existing service identities were preserved");
    }
    tokio::fs::rename(stage, &config).await?;
    if let Some(process) = child.as_mut() {
        let pid = process
            .id()
            .context("Website Tor exited during configuration")?;
        // Signal only the child we own. No system service operation or global
        // Tor reload is involved. HUP preserves active unrelated site circuits.
        let sent = Command::new("kill")
            .args(["-HUP", &pid.to_string()])
            .status()
            .await?;
        if !sent.success() {
            bail!("Could not reload website Tor");
        }
    } else {
        *child = Some(
            Command::new("tor")
                .args(["--defaults-torrc", "/dev/null", "-f"])
                .arg(&config)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .context("Install the open-source Tor package to publish onion websites")?,
        );
    }
    *previous = next;
    Ok(())
}
async fn onion(root: &Path, id: &str) -> Option<String> {
    let address =
        tokio::fs::read_to_string(root.join("publishing/onions").join(id).join("hostname"))
            .await
            .ok()?;
    let address = address.trim();
    let key = address.strip_suffix(".onion")?;
    (key.len() == 56
        && key
            .bytes()
            .all(|c| c.is_ascii_lowercase() || (b'2'..=b'7').contains(&c)))
    .then(|| address.to_owned())
}

pub async fn run(root: PathBuf, mut shutdown: watch::Receiver<bool>) {
    let mut child: Option<Child> = None;
    let mut previous = String::new();
    let mut listeners: BTreeMap<String, (u16, tokio::task::AbortHandle)> = BTreeMap::new();
    let mut tasks = JoinSet::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! { _ = shutdown.changed() => break, _ = tick.tick() => {} }
        while tasks.try_join_next().is_some() {}
        let guard = super::WRITE_LOCK.lock().await;
        let state = match super::load(&root).await {
            Ok(state) => state,
            Err(e) => {
                tasks.abort_all();
                listeners.clear();
                if let Some(mut process) = child.take() {
                    let _ = process.kill().await;
                }
                previous.clear();
                serving::replace_snapshot(State::default()).await;
                *STATUS.write().await = vec![TorStatus {
                    project_id: String::new(),
                    onion_address: None,
                    listening: false,
                    externally_verified: false,
                    error: Some(e.to_string()),
                }];
                continue;
            }
        };
        serving::replace_snapshot(state.clone()).await;
        drop(guard);
        let desired: BTreeMap<String, u16> = state
            .projects
            .iter()
            .filter_map(|(id, p)| Some((id.clone(), p.tor_publication.as_ref()?.port)))
            .collect();
        listeners.retain(|id, (port, task)| {
            let keep = desired.get(id) == Some(port) && !task.is_finished();
            if !keep {
                task.abort();
            }
            keep
        });
        let mut statuses = vec![];
        for (id, port) in &desired {
            let mut error = None;
            if !listeners.contains_key(id) {
                match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, *port)).await {
                    Ok(listener) => {
                        let task =
                            tasks.spawn(serving::listen(listener, id.clone(), *port, Route::Tor));
                        listeners.insert(id.clone(), (*port, task));
                    }
                    Err(e) => error = Some(format!("Onion website listener unavailable: {e}")),
                }
            }
            statuses.push(TorStatus {
                project_id: id.clone(),
                onion_address: onion(&root, id).await,
                listening: listeners.contains_key(id),
                externally_verified: false,
                error,
            });
        }
        let available = listeners
            .iter()
            .map(|(id, (port, _))| (id.clone(), *port))
            .collect();
        if let Err(e) = configure(&root, &available, &mut child, &mut previous).await {
            tasks.abort_all();
            listeners.clear();
            for status in &mut statuses {
                status.listening = false;
                status.error = Some(e.to_string());
            }
        }
        *STATUS.write().await = statuses;
    }
    tasks.abort_all();
    if let Some(mut process) = child {
        let _ = process.kill().await;
    }
    STATUS.write().await.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_has_no_proxy_and_only_owned_local_ports() {
        let id = "05236631-1e6d-4f1b-bdef-32a208f5fe89".to_owned();
        let config = render(
            Path::new("/tmp/website-tests"),
            &[(id.clone(), 32100)].into_iter().collect(),
        )
        .unwrap();
        assert!(config.contains("SocksPort 0\nControlPort 0\nRunAsDaemon 0"));
        assert!(config.contains("HiddenServicePort 80 127.0.0.1:32100"));
        assert!(render(
            Path::new("/tmp/website-tests"),
            &[(id, 8332)].into_iter().collect()
        )
        .is_err());
        assert!(render(
            Path::new("/tmp/website-tests"),
            &[("../wallet".into(), 32100)].into_iter().collect()
        )
        .is_err());
        assert!(render(Path::new("/tmp/bad\npath"), &BTreeMap::new()).is_err());
    }
}
