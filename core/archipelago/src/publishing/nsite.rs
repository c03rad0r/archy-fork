//! NIP-5A named-site preparation only. Upload and explicit identity signing use
//! the dashboard's existing signer; this module never exports or creates keys.
use super::{Project, Route};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const POLICY: &str = "<!doctype html><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'\"><meta name=\"referrer\" content=\"no-referrer\">";

pub fn local_server(project: &Project, raw: &str) -> Result<String> {
    let server = server(raw)?;
    anyhow::ensure!(
        project.routes.contains(&Route::Nostr)
            && project.routes.contains(&Route::PublicWeb)
            && project.fips_publication.is_some(),
        "Publish this website over public HTTPS before using local Blossom for an nsite"
    );
    let domain = project
        .domain
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Set the website domain first"))?;
    anyhow::ensure!(
        server == format!("https://{}", domain.hostname),
        "Local nsite assets must use this website’s HTTPS origin"
    );
    Ok(server)
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn server(raw: &str) -> Result<String> {
    let value = raw.trim().trim_end_matches('/');
    let host = value
        .strip_prefix("https://")
        .ok_or_else(|| anyhow::anyhow!("Enter an HTTPS Blossom server origin"))?;
    Ok(format!("https://{}", super::hostname(host)?))
}
pub fn prepare(project: &Project, blossom: &str) -> Result<Value> {
    if !project.routes.contains(&Route::Nostr) || project.draft.trim().is_empty() {
        bail!("Save a website draft and select Nostr before publishing");
    }
    let server = server(blossom)?;
    // The first policy remains restrictive even if generated HTML adds another
    // CSP. Hosted nsites use a separate origin, without dashboard privileges.
    let html = format!("{POLICY}{}", project.draft);
    anyhow::ensure!(
        html.len() <= super::MAX_HTML,
        "Prepared website exceeds 512 KiB"
    );
    let digest = hash(html.as_bytes());
    let identifier: String = project.id.chars().filter(|c| *c != '-').take(13).collect();
    let aggregate = hash(format!("{digest} /index.html\n").as_bytes());
    let now = chrono::Utc::now().timestamp();
    Ok(json!({
        "html":html, "sha256":digest, "server":server, "identifier":identifier,
        "authorization": { "kind":24242, "created_at":now, "content":"Upload this website's index.html", "tags":[["t","upload"],["x",digest],["server",server.trim_start_matches("https://")],["expiration",(now+300).to_string()]] },
        "manifest": { "kind":35128, "created_at":now, "content":"", "tags":[["d",identifier],["path","/index.html",digest],["x",aggregate,"aggregate"],["server",server],["title",project.name]] }
    }))
}

/// A client-side delivery receipt, not a claim of gateway reachability or of
/// erasure from relays. Keep the signed event so interrupted sends can be retried.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub identity_id: String,
    pub server: String,
    pub event: Value,
    pub accepted_relays: Vec<String>,
    pub deletion_requested: bool,
}
impl Receipt {
    pub fn validate(&self, project_id: &str) -> Result<()> {
        if self.identity_id.is_empty()
            || self.identity_id.len() > 200
            || self.accepted_relays.len() > 8
            || serde_json::to_vec(&self.event)?.len() > 16 * 1024
        {
            bail!("Invalid nsite receipt");
        }
        server(&self.server)?;
        for key in ["id", "pubkey"] {
            let s = self.event[key].as_str().unwrap_or("");
            if s.len() != 64 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
                bail!("Invalid signed nsite event");
            }
        }
        if self.event["kind"] != 35128 {
            bail!("Only named nsite receipts are supported");
        }
        let identifier: String = project_id.chars().filter(|c| *c != '-').take(13).collect();
        let tags = self.event["tags"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing nsite tags"))?;
        if tags.iter().filter(|t| t[0] == "d").count() != 1
            || !tags.iter().any(|t| t == &json!(["d", identifier]))
        {
            bail!("Nsite receipt does not belong to this project");
        }
        if self
            .accepted_relays
            .iter()
            .any(|r| !r.starts_with("wss://") || r.len() > 300 || r.chars().any(char::is_control))
        {
            bail!("Invalid relay receipt");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::publishing::{Change, State};
    #[test]
    fn named_manifest_scopes_auth_and_hashes_exact_uploaded_bytes() {
        let mut state = State::default();
        let id = state
            .apply(Change::Create {
                name: "Site".into(),
            })
            .unwrap()
            .unwrap();
        state
            .apply(Change::Save {
                id: id.clone(),
                name: "Site".into(),
                routes: [Route::Nostr].into_iter().collect(),
                domain: None,
                html: "<h1>Hello 🏝</h1>".into(),
            })
            .unwrap();
        let p = prepare(&state.projects[&id], "https://blossom.example.org/").unwrap();
        assert_eq!(p["sha256"], hash(p["html"].as_str().unwrap().as_bytes()));
        assert_eq!(p["manifest"]["kind"], 35128);
        assert_eq!(p["manifest"]["tags"][0][1].as_str().unwrap().len(), 13);
        assert_eq!(
            p["authorization"]["tags"][2],
            json!(["server", "blossom.example.org"])
        );
        assert!(p["html"]
            .as_str()
            .unwrap()
            .starts_with("<!doctype html><meta http-equiv=\"Content-Security-Policy\""));
        for bad in [
            "http://example.org",
            "https://127.0.0.1",
            "https://user:secret@example.org",
            "https://example.org/path",
        ] {
            assert!(server(bad).is_err());
        }
    }
}
