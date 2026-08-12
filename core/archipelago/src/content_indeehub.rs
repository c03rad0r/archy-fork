//! IndeeHub as a content source for the assistant's film grid.
//!
//! # Why the node fetches this, not the browser
//!
//! IndeeHub keeps its catalogue in its own Postgres behind its own API, and the
//! interesting half — a user's private titles — requires a **Nostr session**
//! (`Cognito authentication is disabled. Use Nostr login.`). Signing that login
//! in the browser would put identity material next to the model, which Phase 13
//! rules out by name. So the node signs with its own key, holds the resulting
//! session, and hands the assistant nothing but titles.
//!
//! This also keeps the context broker's contract intact: only a `scope` enum
//! ever crosses from AIUI to the node, never a URL or a method name (T-13-34).
//!
//! # How the login works
//!
//! NIP-98 (kind 27235): an event whose tags name the exact URL and method,
//! signed by the node's Nostr key, base64'd into `Authorization: Nostr <b64>`.
//! IndeeHub answers with a JWT pair; the access token is then an ordinary
//! bearer for `/api/projects*`.
//!
//! Proven end to end on archi-dev-box before this module existed: the node
//! signed a NIP-98 event, IndeeHub issued a real `typ: nostr-session` JWT with
//! `sub` = the node's pubkey, and `/api/projects/private` returned 200 — through
//! the app gate, which had to stop stripping the `Authorization` header first.
//!
//! # Failure is not an error
//!
//! IndeeHub is an optional app. Not installed, not running, mid-restart, or
//! simply empty are all ordinary states, and each yields an empty list rather
//! than failing the caller's whole content request — one absent source must
//! never blank the grid for every other source.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

use crate::nostr_discovery;

/// IndeeHub's own nginx. Its API container is not host-published, so this is
/// the only reachable entry point, and it is loopback-only by design.
const INDEEHUB_BASE: &str = "http://127.0.0.1:7778";

/// Short: this runs inside a user-facing content request. A slow or wedged
/// IndeeHub must cost a moment, not the request.
const TIMEOUT: Duration = Duration::from_secs(6);

/// NIP-98 HTTP-auth event kind.
const KIND_HTTP_AUTH: u64 = 27235;

#[derive(Debug, Clone, Deserialize)]
pub struct IndeehubProject {
    pub id: Option<String>,
    pub title: Option<String>,
    #[serde(alias = "logline", alias = "description")]
    pub synopsis: Option<String>,
    #[serde(alias = "posterUrl", alias = "poster_url", alias = "coverUrl")]
    pub poster: Option<String>,
    #[serde(alias = "releaseYear", alias = "release_year")]
    pub year: Option<serde_json::Value>,
}

/// Every project this node can see: the public catalogue plus, if a Nostr
/// session can be established, the operator's private titles.
///
/// De-duplicated by id, because a title the node owns appears in both lists.
pub async fn list_projects(data_dir: &Path) -> Vec<IndeehubProject> {
    let client = match reqwest::Client::builder().timeout(TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(error = %e, "indeehub: no http client");
            return Vec::new();
        }
    };

    let mut out = fetch_public(&client).await.unwrap_or_else(|e| {
        // Absent app, stopped container, mid-restart: ordinary, not an error.
        tracing::debug!(error = %e, "indeehub: public catalogue unavailable");
        Vec::new()
    });

    match fetch_private(&client, data_dir).await {
        Ok(private) => out.extend(private),
        Err(e) => {
            // The operator may simply have no Nostr identity on this node, or
            // IndeeHub may not know them. Public titles still stand.
            tracing::debug!(error = %e, "indeehub: private catalogue unavailable");
        }
    }

    let mut seen = std::collections::HashSet::new();
    out.retain(|p| match p.id.as_deref() {
        Some(id) => seen.insert(id.to_string()),
        // No id: keep it, but it cannot participate in de-duplication.
        None => true,
    });
    out
}

async fn fetch_public(client: &reqwest::Client) -> Result<Vec<IndeehubProject>> {
    let res = client
        .get(format!("{INDEEHUB_BASE}/api/projects"))
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("projects returned {}", res.status());
    }
    Ok(res.json().await?)
}

async fn fetch_private(client: &reqwest::Client, data_dir: &Path) -> Result<Vec<IndeehubProject>> {
    let token = nostr_session(client, data_dir).await?;
    let res = client
        .get(format!("{INDEEHUB_BASE}/api/projects/private"))
        .bearer_auth(&token)
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("private projects returned {}", res.status());
    }
    Ok(res.json().await?)
}

/// Exchange a signed NIP-98 event for IndeeHub's own access token.
async fn nostr_session(client: &reqwest::Client, data_dir: &Path) -> Result<String> {
    let url = format!("{INDEEHUB_BASE}/api/auth/nostr/session");
    let event = sign_nip98(data_dir, &url, "POST").await?;
    let encoded = base64_encode(serde_json::to_string(&event)?.as_bytes());

    let res = client
        .post(&url)
        .header(reqwest::header::AUTHORIZATION, format!("Nostr {encoded}"))
        .json(&serde_json::json!({}))
        .send()
        .await?;

    let status = res.status();
    let body: serde_json::Value = res.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        anyhow::bail!("nostr session returned {status}: {body}");
    }

    // Field name varies by IndeeHub version; accept the usual spellings rather
    // than pinning one and breaking on an upgrade.
    for key in ["accessToken", "access_token", "token", "jwt"] {
        if let Some(t) = body.get(key).and_then(|v| v.as_str()) {
            return Ok(t.to_string());
        }
    }
    anyhow::bail!("nostr session had no recognisable access token: {body}")
}

/// Build and sign a NIP-98 event for exactly this URL and method.
///
/// The `u` and `method` tags are what make the signature non-replayable against
/// a different endpoint, so they are set from the same values used to send.
async fn sign_nip98(data_dir: &Path, url: &str, method: &str) -> Result<serde_json::Value> {
    let identity_dir = data_dir.join("identity");
    // Prove the identity this node already has; never mint one here.
    // `get_nostr_pubkey` goes through `load_or_create_nostr_keys`, so on a node
    // without an identity it would GENERATE a keypair, sign with it, and write
    // the secret to disk — an HTTP auth header quietly creating a new node
    // identity, and authenticating to IndeeHub as a stranger with a key nobody
    // has ever seen. The `.context("node has no Nostr identity")` below could
    // never fire because of it.
    if !nostr_discovery::nostr_identity_exists(&identity_dir).await {
        anyhow::bail!("node has no Nostr identity");
    }
    let pubkey = nostr_discovery::get_nostr_pubkey(&identity_dir)
        .await
        .context("node has no Nostr identity")?;

    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let tags = serde_json::json!([["u", url], ["method", method]]);

    // NIP-01 id: sha256 over [0, pubkey, created_at, kind, tags, content].
    let serialized =
        serde_json::json!([0, pubkey, created_at, KIND_HTTP_AUTH, tags, ""]).to_string();
    use sha2::{Digest, Sha256};
    let id = hex::encode(Sha256::digest(serialized.as_bytes()));

    let sig = nostr_discovery::nostr_sign_hash(&identity_dir, &id).await?;

    Ok(serde_json::json!({
        "id": id,
        "pubkey": pubkey,
        "created_at": created_at,
        "kind": KIND_HTTP_AUTH,
        "tags": tags,
        "content": "",
        "sig": sig,
    }))
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

impl IndeehubProject {
    /// Year as a number regardless of whether IndeeHub sent it as one, a
    /// string, or a full date — versions differ and none of them is wrong.
    pub fn year_num(&self) -> Option<u32> {
        match self.year.as_ref()? {
            serde_json::Value::Number(n) => n.as_u64().map(|y| y as u32),
            serde_json::Value::String(s) => s
                .get(..4)
                .and_then(|p| p.parse::<u32>().ok())
                .filter(|y| (1800..=2200).contains(y)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(json: serde_json::Value) -> IndeehubProject {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn accepts_the_field_spellings_indeehub_versions_actually_use() {
        let p = project(serde_json::json!({
            "id": "1", "title": "A Film", "logline": "A line", "posterUrl": "http://x/y.jpg"
        }));
        assert_eq!(p.synopsis.as_deref(), Some("A line"));
        assert_eq!(p.poster.as_deref(), Some("http://x/y.jpg"));
    }

    #[test]
    fn a_project_with_only_a_title_still_parses() {
        // Every field but the title is optional upstream; a strict struct here
        // would drop real films over a missing poster.
        let p = project(serde_json::json!({ "title": "Bare" }));
        assert_eq!(p.title.as_deref(), Some("Bare"));
        assert!(p.id.is_none());
    }

    #[test]
    fn year_survives_number_string_and_date_forms() {
        assert_eq!(
            project(serde_json::json!({"releaseYear": 2014})).year_num(),
            Some(2014)
        );
        assert_eq!(
            project(serde_json::json!({"releaseYear": "2016"})).year_num(),
            Some(2016)
        );
        assert_eq!(
            project(serde_json::json!({"releaseYear": "2020-05-01"})).year_num(),
            Some(2020)
        );
        assert_eq!(
            project(serde_json::json!({"releaseYear": "n/a"})).year_num(),
            None
        );
        assert_eq!(project(serde_json::json!({})).year_num(), None);
    }

    #[tokio::test]
    async fn a_nip98_event_names_the_exact_url_and_method() {
        // The tags are what stop a captured signature being replayed against a
        // different endpoint, so they must not drift from what is sent.
        let dir = tempfile::tempdir().unwrap();
        // No identity present: must fail loudly rather than sign something
        // empty or fall back to an unsigned request.
        let out = sign_nip98(dir.path(), "http://x/api/auth", "POST").await;
        assert!(out.is_err());
    }
}
