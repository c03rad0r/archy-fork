//! Node-owned publishing drafts. Saving intent never opens a listener or claims
//! reachability. Transport adapters must supply independent live evidence.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tokio::sync::Mutex;

mod firewall;
pub mod nsite;
pub mod serving;
pub mod tor;

static WRITE_LOCK: Mutex<()> = Mutex::const_new(());
const MAX_STATE: usize = 16 * 1024 * 1024;
const MAX_HTML: usize = 512 * 1024;
const MAX_PROJECTS: usize = 32;
const MAX_REVISIONS: usize = 20;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Route {
    Fips,
    PublicWeb,
    Tor,
    Nostr,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Domain {
    pub hostname: String,
    pub destination: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub routes: BTreeSet<Route>,
    pub domain: Option<Domain>,
    pub draft: String,
    pub revisions: Vec<Revision>,
    #[serde(default)]
    pub fips_publication: Option<Publication>,
    #[serde(default)]
    pub tor_publication: Option<Publication>,
    #[serde(default)]
    pub nsite_receipt: Option<nsite::Receipt>,
    #[serde(default)]
    pub local_archive: Option<LocalArchive>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalArchive {
    pub sha256: String,
    pub size: usize,
    pub pubkey: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub port: u16,
    pub html: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub id: String,
    pub created_at: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema: u32,
    pub version: u64,
    pub connections: BTreeSet<Route>,
    pub projects: BTreeMap<String, Project>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            schema: 1,
            version: 0,
            connections: BTreeSet::new(),
            projects: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    // Only the local storage adapter can claim a verified archive receipt.
    #[serde(skip_deserializing)]
    RecordLocalArchive {
        id: String,
        receipt: LocalArchive,
    },
    RecordNsite {
        id: String,
        receipt: nsite::Receipt,
    },
    Connections {
        routes: BTreeSet<Route>,
    },
    Create {
        name: String,
    },
    Save {
        id: String,
        name: String,
        routes: BTreeSet<Route>,
        domain: Option<Domain>,
        html: String,
    },
    PublishFips {
        id: String,
        acknowledge_public: bool,
    },
    PublishTor {
        id: String,
        acknowledge_public: bool,
    },
    UnpublishTor {
        id: String,
    },
    UnpublishFips {
        id: String,
    },
    Restore {
        id: String,
        revision: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub version: u64,
    pub change: Change,
}

/// ASCII DNS names only; callers may enter an IDNA A-label. No URLs, wildcards,
/// ports, path fragments, or private overlay suffixes as public domain names.
pub fn hostname(value: &str) -> Result<String> {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if value.len() > 253
        || !value.contains('.')
        || value.parse::<std::net::IpAddr>().is_ok()
        || [".fips", ".onion", ".local", ".localhost", ".internal"]
            .iter()
            .any(|s| value.ends_with(s))
        || !value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    {
        bail!("Enter a public domain name, without a protocol, port or path");
    }
    Ok(value)
}

fn name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 100 || value.chars().any(char::is_control) {
        bail!("Website name must contain 1–100 characters without control characters");
    }
    Ok(value.to_owned())
}

pub(crate) fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(a) => {
            let o = a.octets();
            !a.is_private()
                && !a.is_loopback()
                && !a.is_link_local()
                && !a.is_multicast()
                && !a.is_unspecified()
                && !a.is_broadcast()
                && !a.is_documentation()
                && o[0] != 0
                && o[0] < 240
                && !(o[0] == 100 && (64..=127).contains(&o[1]))
                && !(o[0] == 198 && (o[1] == 18 || o[1] == 19))
                && !(o[0] == 192 && o[1] == 0 && o[2] == 0)
        }
        std::net::IpAddr::V6(a) => {
            let s = a.segments();
            // Only global unicast; excludes ULA/FIPS, mapped-v4, loopback,
            // multicast and link-local, plus documentation allocations.
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && s[1] < 0x200)
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && s[1] == 0x0db8)
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DnsRecord {
    pub record_type: &'static str,
    pub name: String,
    pub value: String,
    pub ttl: u32,
}

pub fn dns_records(domain: &Domain) -> Result<Vec<DnsRecord>> {
    let host = hostname(&domain.hostname)?;
    let Some(raw) = &domain.destination else {
        return Ok(vec![]);
    };
    let target = raw.trim();
    if target.is_empty() {
        return Ok(vec![]);
    }
    let (record_type, value) = match target.parse::<std::net::IpAddr>() {
        Ok(ip) => {
            if !public_ip(ip) {
                bail!("Use the gateway's public IP or a verified public node IP; private and FIPS addresses are not public web destinations");
            }
            (if ip.is_ipv4() { "A" } else { "AAAA" }, ip.to_string())
        }
        Err(_) => {
            let target = hostname(target)?;
            if target == host {
                bail!("A domain cannot point to itself with a CNAME");
            }
            ("CNAME", target)
        }
    };
    Ok(vec![DnsRecord {
        record_type,
        name: host,
        value,
        ttl: 3600,
    }])
}

impl State {
    pub fn apply(&mut self, change: Change) -> Result<Option<String>> {
        match change {
            Change::RecordLocalArchive { id, receipt } => {
                let p = self
                    .projects
                    .get_mut(&id)
                    .context("Website project not found")?;
                if receipt.sha256 != nsite::hash(p.draft.as_bytes())
                    || receipt.size != p.draft.len()
                {
                    bail!("The draft changed while storing it. The stored file is retained; review the current draft");
                }
                p.local_archive = Some(receipt);
                Ok(Some(id))
            }
            Change::RecordNsite { id, receipt } => {
                receipt.validate(&id)?;
                let p = self
                    .projects
                    .get_mut(&id)
                    .context("Website project not found")?;
                p.nsite_receipt = Some(receipt);
                Ok(Some(id))
            }
            Change::Connections { routes } => {
                self.connections = routes;
                Ok(None)
            }
            Change::Create { name: raw } => {
                if self.projects.len() >= MAX_PROJECTS {
                    bail!("Maximum number of website projects reached");
                }
                let name = name(&raw)?;
                let id = uuid::Uuid::new_v4().to_string();
                self.projects.insert(
                    id.clone(),
                    Project {
                        id: id.clone(),
                        name,
                        routes: self.connections.clone(),
                        domain: None,
                        draft: String::new(),
                        revisions: vec![],
                        fips_publication: None,
                        tor_publication: None,
                        nsite_receipt: None,
                        local_archive: None,
                    },
                );
                Ok(Some(id))
            }
            Change::Save {
                id,
                name: raw,
                routes,
                mut domain,
                html,
            } => {
                let name = name(&raw)?;
                if html.len() > MAX_HTML || html.contains('\0') {
                    bail!("Website HTML must be at most 512 KiB and contain no NUL bytes");
                }
                if let Some(d) = domain.as_mut() {
                    d.hostname = hostname(&d.hostname)?;
                    if !routes.contains(&Route::PublicWeb) && !routes.contains(&Route::Nostr) {
                        bail!("A public domain requires public web or an nsite gateway");
                    }
                    dns_records(d)?;
                }
                let p = self
                    .projects
                    .get_mut(&id)
                    .context("Website project not found")?;
                if p.fips_publication.is_some()
                    && !routes.contains(&Route::Fips)
                    && !routes.contains(&Route::PublicWeb)
                {
                    bail!("Unpublish the FIPS website before removing its route");
                }
                if p.tor_publication.is_some() && !routes.contains(&Route::Tor) {
                    bail!("Unpublish the onion website before removing its route");
                }
                if p.draft != html {
                    p.revisions.push(Revision {
                        id: uuid::Uuid::new_v4().to_string(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                        html: html.clone(),
                    });
                    if p.revisions.len() > MAX_REVISIONS {
                        p.revisions.remove(0);
                    }
                }
                p.name = name;
                p.routes = routes;
                p.domain = domain;
                p.draft = html;
                Ok(Some(id))
            }
            Change::PublishFips {
                id,
                acknowledge_public,
            } => {
                if !acknowledge_public {
                    bail!("Confirm that anyone with a FIPS route may view this website");
                }
                let used: BTreeSet<u16> = self
                    .projects
                    .values()
                    .filter_map(|p| p.fips_publication.as_ref().map(|p| p.port))
                    .collect();
                let p = self
                    .projects
                    .get_mut(&id)
                    .context("Website project not found")?;
                if (!p.routes.contains(&Route::Fips) && !p.routes.contains(&Route::PublicWeb))
                    || p.draft.trim().is_empty()
                {
                    bail!("Save a website draft and select FIPS or public web before publishing");
                }
                let port = match &p.fips_publication {
                    Some(old) => old.port,
                    None => (32000..32032)
                        .find(|port| !used.contains(port))
                        .context("No website ports available")?,
                };
                p.fips_publication = Some(Publication {
                    port,
                    html: p.draft.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });
                Ok(Some(id))
            }
            Change::PublishTor {
                id,
                acknowledge_public,
            } => {
                if !acknowledge_public {
                    bail!("Confirm that anyone with the onion address may view this website");
                }
                let used: BTreeSet<u16> = self
                    .projects
                    .values()
                    .filter_map(|p| p.tor_publication.as_ref().map(|p| p.port))
                    .collect();
                let p = self
                    .projects
                    .get_mut(&id)
                    .context("Website project not found")?;
                if !p.routes.contains(&Route::Tor) || p.draft.trim().is_empty() {
                    bail!("Save a website draft and select Tor before publishing");
                }
                let port = match &p.tor_publication {
                    Some(old) => old.port,
                    None => (32100..32132)
                        .find(|port| !used.contains(port))
                        .context("No onion website ports available")?,
                };
                p.tor_publication = Some(Publication {
                    port,
                    html: p.draft.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });
                Ok(Some(id))
            }
            Change::UnpublishTor { id } => {
                self.projects
                    .get_mut(&id)
                    .context("Website project not found")?
                    .tor_publication = None;
                Ok(Some(id))
            }
            Change::UnpublishFips { id } => {
                self.projects
                    .get_mut(&id)
                    .context("Website project not found")?
                    .fips_publication = None;
                Ok(Some(id))
            }
            Change::Restore { id, revision } => {
                let p = self
                    .projects
                    .get_mut(&id)
                    .context("Website project not found")?;
                let previous = p
                    .revisions
                    .iter()
                    .find(|r| r.id == revision)
                    .context("Website revision not found")?
                    .html
                    .clone();
                p.draft = previous;
                Ok(Some(id))
            }
        }
    }
}

pub async fn load(root: &Path) -> Result<State> {
    let path = root.join("publishing/state.json");
    if let Ok(meta) = tokio::fs::metadata(&path).await {
        if meta.len() > MAX_STATE as u64 {
            bail!("Publishing storage limit exceeded; existing state has been preserved");
        }
    }
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(e) => return Err(e.into()),
    };
    let state: State = serde_json::from_slice(&bytes)
        .context("Publishing state is unreadable; existing data has been preserved")?;
    if state.schema != 1 {
        bail!("Unsupported publishing state version; upgrade before making changes");
    }
    let mut ports = BTreeSet::new();
    for (id, project) in &state.projects {
        if project.id != *id
            || uuid::Uuid::parse_str(id)
                .map(|u| u.to_string() != *id)
                .unwrap_or(true)
        {
            bail!("Invalid stored website identity; existing state has been preserved");
        }
        for (publication, range) in [
            (&project.fips_publication, 32000..32032),
            (&project.tor_publication, 32100..32132),
        ] {
            if let Some(p) = publication {
                if !range.contains(&p.port) || !ports.insert(p.port) || p.html.len() > MAX_HTML {
                    bail!("Invalid stored website publication; existing state has been preserved");
                }
            }
        }
    }
    Ok(state)
}

/// Serialize read-modify-write and reject stale browser state. Atomic replacement
/// ensures a failed save cannot leave partial JSON or silently reset projects.
pub async fn update(root: &Path, request: Update) -> Result<(State, Option<String>)> {
    let _guard = WRITE_LOCK.lock().await;
    let mut state = load(root).await?;
    if state.version != request.version {
        bail!("Publishing settings changed in another window. Reload before saving");
    }
    let id = state.apply(request.change)?;
    state.version = state
        .version
        .checked_add(1)
        .context("Publishing version exhausted")?;
    let bytes = serde_json::to_vec_pretty(&state)?;
    if bytes.len() > MAX_STATE {
        bail!(
            "Publishing storage is full (16 MiB). Export older projects before adding more content"
        );
    }
    let dir = root.join("publishing");
    tokio::fs::create_dir_all(&dir).await?;
    let tmp = dir.join(format!("state-{}.tmp", uuid::Uuid::new_v4()));
    let result = async {
        use tokio::io::AsyncWriteExt;
        let mut opts = tokio::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut f = opts.open(&tmp).await?;
        f.write_all(&bytes).await?;
        f.sync_all().await?;
        tokio::fs::rename(&tmp, dir.join("state.json")).await?;
        // Persist the rename as well as the file contents across power loss.
        tokio::fs::File::open(&dir).await?.sync_all().await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    result?;
    serving::replace_snapshot(state.clone()).await;
    Ok((state, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_archive_receipts_cannot_be_claimed_by_clients_or_publish_routes() {
        assert!(serde_json::from_value::<Change>(
            serde_json::json!({"action":"record-local-archive", "id":"x", "receipt":{}})
        )
        .is_err());
        let mut state = State::default();
        let id = state
            .apply(Change::Create {
                name: "Local archive".into(),
            })
            .unwrap()
            .unwrap();
        state.projects.get_mut(&id).unwrap().draft = "<p>Private draft</p>".into();
        let draft = &state.projects[&id].draft;
        let mut receipt = LocalArchive {
            sha256: nsite::hash(draft.as_bytes()),
            size: draft.len(),
            pubkey: "a".repeat(64),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        state
            .apply(Change::RecordLocalArchive {
                id: id.clone(),
                receipt: receipt.clone(),
            })
            .unwrap();
        let p = &state.projects[&id];
        assert!(
            p.routes.is_empty()
                && p.fips_publication.is_none()
                && p.tor_publication.is_none()
                && p.nsite_receipt.is_none()
        );
        receipt.sha256 = "b".repeat(64);
        assert!(state
            .apply(Change::RecordLocalArchive { id, receipt })
            .is_err());
    }
    #[tokio::test]
    async fn concurrent_edit_is_rejected_and_project_survives_reload() {
        let d = tempfile::tempdir().unwrap();
        let (s, id) = update(
            d.path(),
            Update {
                version: 0,
                change: Change::Create {
                    name: "My site".into(),
                },
            },
        )
        .await
        .unwrap();
        assert_eq!(s.version, 1);
        assert!(update(
            d.path(),
            Update {
                version: 0,
                change: Change::Connections {
                    routes: BTreeSet::new()
                }
            }
        )
        .await
        .is_err());
        assert!(load(d.path())
            .await
            .unwrap()
            .projects
            .contains_key(&id.unwrap()));
    }
    #[tokio::test]
    async fn corrupt_state_is_not_replaced() {
        let d = tempfile::tempdir().unwrap();
        tokio::fs::create_dir(d.path().join("publishing"))
            .await
            .unwrap();
        let path = d.path().join("publishing/state.json");
        tokio::fs::write(&path, "broken").await.unwrap();
        assert!(update(
            d.path(),
            Update {
                version: 0,
                change: Change::Create {
                    name: "Site".into()
                }
            }
        )
        .await
        .is_err());
        assert_eq!(tokio::fs::read_to_string(path).await.unwrap(), "broken");
    }
    #[test]
    fn reject_private_targets_and_configuration_injection() {
        for target in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "fd12::1",
            "::1",
            "192.168.1.2",
            "::ffff:8.8.8.8",
            "2002:7f00:1::1",
            "2001::1",
            "192.0.0.1",
            "node.fips",
            "a.onion",
            "example.com; bad",
            "https://example.com",
        ] {
            assert!(
                dns_records(&Domain {
                    hostname: "www.example.com".into(),
                    destination: Some(target.into())
                })
                .is_err(),
                "{target}"
            );
        }
        for host in [
            "../x",
            "*.example.com",
            "example.com:443",
            "a\nb.example.com",
            "-bad.com",
        ] {
            assert!(hostname(host).is_err());
        }
    }
    #[test]
    fn public_web_reuses_fips_upstream_without_requiring_a_second_route_choice() {
        let mut state = State::default();
        state.connections.insert(Route::PublicWeb);
        let id = state
            .apply(Change::Create {
                name: "Public site".into(),
            })
            .unwrap()
            .unwrap();
        state.projects.get_mut(&id).unwrap().draft = "<h1>Public</h1>".into();
        assert!(state
            .apply(Change::PublishFips {
                id: id.clone(),
                acknowledge_public: false
            })
            .is_err());
        state
            .apply(Change::PublishFips {
                id: id.clone(),
                acknowledge_public: true,
            })
            .unwrap();
        assert!(state.projects[&id].fips_publication.is_some());
        assert!(!state.projects[&id].routes.contains(&Route::Fips));
        let save = |routes| Change::Save {
            id: id.clone(),
            name: "Public site".into(),
            routes,
            domain: None,
            html: "<h1>Public</h1>".into(),
        };
        state
            .apply(save([Route::PublicWeb].into_iter().collect()))
            .unwrap();
        assert!(state.apply(save(BTreeSet::new())).is_err());
    }
    #[test]
    fn multiple_routes_and_restore_do_not_publish() {
        let mut s = State::default();
        s.apply(Change::Connections {
            routes: [Route::Fips, Route::PublicWeb].into_iter().collect(),
        })
        .unwrap();
        let id = s
            .apply(Change::Create {
                name: "Site".into(),
            })
            .unwrap()
            .unwrap();
        assert_eq!(s.projects[&id].routes, s.connections);
        assert!(s.projects[&id].fips_publication.is_none());
        let routes = [Route::Fips, Route::Tor, Route::PublicWeb, Route::Nostr]
            .into_iter()
            .collect();
        s.apply(Change::Save {
            id: id.clone(),
            name: "Site".into(),
            routes,
            domain: None,
            html: "<h1>Hello</h1>".into(),
        })
        .unwrap();
        let revision = s.projects[&id].revisions[0].id.clone();
        s.apply(Change::Save {
            id: id.clone(),
            name: "Site".into(),
            routes: BTreeSet::new(),
            domain: None,
            html: "<h1>New</h1>".into(),
        })
        .unwrap();
        s.apply(Change::Restore {
            id: id.clone(),
            revision,
        })
        .unwrap();
        assert_eq!(s.projects[&id].draft, "<h1>Hello</h1>");
        assert_eq!(s.projects[&id].revisions.len(), 2);
        assert_eq!(s.connections.len(), 2);
    }
    #[test]
    fn dns_records_distinguish_ip_and_alias() {
        for (target, kind) in [
            ("8.8.8.8", "A"),
            ("2606:4700:4700::1111", "AAAA"),
            ("gateway.example.org", "CNAME"),
        ] {
            let records = dns_records(&Domain {
                hostname: "www.example.com".into(),
                destination: Some(target.into()),
            })
            .unwrap();
            assert_eq!(records[0].record_type, kind);
            assert_eq!(records[0].name, "www.example.com");
        }
    }
}
