//! A dedicated static-only FIPS origin per website. No dashboard routing,
//! filesystem paths, authentication cookies, proxy targets or AI tools here.
use super::State;
use hyper::{Body, Method, Request, Response, StatusCode};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use tokio::sync::{watch, RwLock, Semaphore};
use tokio::task::JoinSet;

pub const CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; sandbox";
#[derive(Clone, serde::Serialize)]
pub struct ListenerStatus {
    pub project_id: String,
    pub address: Option<String>,
    pub listening: bool,
    pub externally_verified: bool,
    pub error: Option<String>,
}
pub(super) static SNAPSHOT: LazyLock<RwLock<State>> =
    LazyLock::new(|| RwLock::new(State::default()));
pub async fn replace_snapshot(state: State) {
    *SNAPSHOT.write().await = state;
}

static STATUS: LazyLock<RwLock<Vec<ListenerStatus>>> = LazyLock::new(|| RwLock::new(vec![]));
pub async fn status() -> Vec<ListenerStatus> {
    STATUS.read().await.clone()
}

#[cfg(test)]
pub fn response(state: &State, id: &str, port: u16, req: &Request<Body>) -> Response<Body> {
    response_for(state, id, port, super::Route::Fips, req)
}

pub(super) fn response_for(
    state: &State,
    id: &str,
    port: u16,
    route: super::Route,
    req: &Request<Body>,
) -> Response<Body> {
    let Some(publication) = state
        .projects
        .get(id)
        .and_then(|p| match route {
            super::Route::Fips => p.fips_publication.as_ref(),
            super::Route::Tor => p.tor_publication.as_ref(),
            _ => None,
        })
        .filter(|p| p.port == port)
    else {
        return simple(StatusCode::NOT_FOUND, "Website is not published");
    };
    // Bind managed gateway routes to the project, even if a freed listener port
    // is later assigned to a different published website.
    if req
        .headers()
        .get("x-archipelago-website")
        .is_some_and(|v| v.to_str().ok() != Some(id))
    {
        return simple(StatusCode::NOT_FOUND, "Website route no longer matches");
    }
    // Only the selected immutable snapshot is exposed, never the Blossom backend.
    // No listing, upload, arbitrary hash lookup, filesystem access or credentials.
    let nsite_asset = publication.nsite_asset.as_ref().filter(|asset| {
        req.uri().path() == format!("/{}", asset.receipt.sha256)
            && asset.receipt.sha256 == super::nsite::hash(asset.html.as_bytes())
            && asset.receipt.size == asset.html.len()
    });
    let html = nsite_asset
        .map(|asset| asset.html.as_str())
        .unwrap_or(&publication.html);
    let asset = nsite_asset.is_some()
        || publication.public_archive.as_ref().is_some_and(|hash| {
            req.uri().path() == format!("/{hash}")
                && *hash == super::nsite::hash(publication.html.as_bytes())
        });
    if asset && req.method() == Method::OPTIONS {
        let mut response = simple(StatusCode::NO_CONTENT, "");
        asset_headers(&mut response);
        return response;
    }
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return simple(
            StatusCode::METHOD_NOT_ALLOWED,
            "Only GET and HEAD are supported",
        );
    }
    if !asset && !matches!(req.uri().path(), "/" | "/index.html") {
        return simple(StatusCode::NOT_FOUND, "Not found");
    }
    let mut response = simple(StatusCode::OK, "");
    response
        .headers_mut()
        .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
    response
        .headers_mut()
        .insert("content-length", html.len().to_string().parse().unwrap());
    if asset {
        asset_headers(&mut response);
    }
    if req.method() == Method::GET {
        *response.body_mut() = Body::from(html.to_owned());
    }
    response
}
fn asset_headers(response: &mut Response<Body>) {
    for (name, value) in [
        ("access-control-allow-origin", "*"),
        ("access-control-allow-methods", "GET, HEAD, OPTIONS"),
        (
            "access-control-expose-headers",
            "Content-Length, Content-Type",
        ),
        ("content-disposition", "attachment; filename=\"index.html\""),
    ] {
        response.headers_mut().insert(name, value.parse().unwrap());
    }
}
fn simple(status: StatusCode, body: &str) -> Response<Body> {
    let mut r = Response::new(Body::from(body.to_owned()));
    *r.status_mut() = status;
    for (name, value) in [
        ("content-type", "text/plain; charset=utf-8"),
        ("content-security-policy", CSP),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
        ("connection", "close"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        ),
    ] {
        r.headers_mut().insert(name, value.parse().unwrap());
    }
    r
}

pub async fn run(root: PathBuf, mut shutdown: watch::Receiver<bool>) {
    // JoinSet ownership guarantees that removing a listener or stopping the
    // supervisor also cancels its bounded in-flight HTTP tasks.
    let mut listeners: BTreeMap<String, (SocketAddr, tokio::task::AbortHandle)> = BTreeMap::new();
    let mut tasks = JoinSet::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tick.tick() => {},
        }
        while tasks.try_join_next().is_some() {}
        // Serialize loading and snapshot replacement with RPC writes. A missing
        // or restored state file must revoke the old in-memory publication,
        // even when its version is lower than the previous snapshot.
        let guard = super::WRITE_LOCK.lock().await;
        let state = match super::load(&root).await {
            Ok(s) => s,
            Err(e) => {
                tasks.abort_all();
                listeners.clear();
                *SNAPSHOT.write().await = State::default();
                *STATUS.write().await = vec![ListenerStatus {
                    project_id: String::new(),
                    address: None,
                    listening: false,
                    externally_verified: false,
                    error: Some(e.to_string()),
                }];
                continue;
            }
        };
        replace_snapshot(state.clone()).await;
        drop(guard);
        let ip = crate::fips::iface::fips0_ula();
        let desired: BTreeMap<_, _> = state
            .projects
            .iter()
            .filter_map(|(id, p)| {
                Some((
                    id.clone(),
                    SocketAddr::new(ip?.into(), p.fips_publication.as_ref()?.port),
                ))
            })
            .collect();
        listeners.retain(|id, (addr, task)| {
            let keep = desired.get(id) == Some(addr) && !task.is_finished();
            if !keep {
                task.abort();
            }
            keep
        });
        let mut statuses = vec![];
        for (id, p) in &state.projects {
            let Some(publication) = &p.fips_publication else {
                continue;
            };
            let Some(addr) = desired.get(id).copied() else {
                statuses.push(ListenerStatus {
                    project_id: id.clone(),
                    address: None,
                    listening: false,
                    externally_verified: false,
                    error: Some("FIPS has no local IPv6 address; publication is waiting".into()),
                });
                continue;
            };
            let mut error = None;
            if !listeners.contains_key(id) {
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(listener) => {
                        let project_id = id.clone();
                        let port = publication.port;
                        let task =
                            tasks.spawn(listen(listener, project_id, port, super::Route::Fips));
                        listeners.insert(id.clone(), (addr, task));
                    }
                    Err(e) => error = Some(format!("Website listener unavailable: {e}")),
                }
            }
            statuses.push(ListenerStatus {
                project_id: id.clone(),
                address: Some(format!("http://{addr}/")),
                listening: listeners.contains_key(id),
                externally_verified: false,
                error,
            });
        }
        let ports = listeners.values().map(|(addr, _)| addr.port()).collect();
        if let Err(e) = super::firewall::reconcile(&root, &ports).await {
            tasks.abort_all();
            listeners.clear();
            for status in &mut statuses {
                status.listening = false;
                status.error = Some(format!("FIPS firewall not ready: {e}"));
            }
        }
        *STATUS.write().await = statuses;
    }
    tasks.abort_all();
    STATUS.write().await.clear();
}

pub(super) async fn listen(
    listener: tokio::net::TcpListener,
    project_id: String,
    port: u16,
    route: super::Route,
) {
    let permits = Arc::new(Semaphore::new(32));
    let mut requests = JoinSet::new();
    loop {
        while requests.try_join_next().is_some() {}
        let Ok((socket, _)) = listener.accept().await else {
            break;
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            drop(socket);
            continue;
        };
        let id = project_id.clone();
        requests.spawn(async move {
            let _permit = permit;
            let service = hyper::service::service_fn(move |req| {
                let id = id.clone();
                async move {
                    let state = SNAPSHOT.read().await;
                    Ok::<_, std::convert::Infallible>(response_for(&state, &id, port, route, &req))
                }
            });
            let mut http = hyper::server::conn::Http::new();
            http.http1_only(true)
                .http1_keep_alive(false)
                .max_buf_size(8192);
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                http.serve_connection(socket, service),
            )
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn local_nsite_shares_only_reviewed_bytes_and_revokes_independently() {
        use crate::publishing::{Change, Domain, LocalArchive, Route};
        let mut state = State::default();
        let id = state
            .apply(Change::Create {
                name: "Nsite".into(),
            })
            .unwrap()
            .unwrap();
        state
            .apply(Change::Save {
                id: id.clone(),
                name: "Nsite".into(),
                routes: [Route::PublicWeb, Route::Nostr].into_iter().collect(),
                domain: Some(Domain {
                    hostname: "site.example.org".into(),
                    destination: None,
                }),
                html: "<h1>Original</h1>".into(),
            })
            .unwrap();
        state
            .apply(Change::PublishFips {
                id: id.clone(),
                acknowledge_public: true,
            })
            .unwrap();
        let port = state.projects[&id].fips_publication.as_ref().unwrap().port;
        let html = format!("{}<h1>Reviewed</h1>", crate::publishing::nsite::POLICY);
        let hash = crate::publishing::nsite::hash(html.as_bytes());
        let receipt = LocalArchive {
            sha256: hash.clone(),
            size: html.len(),
            pubkey: "a".repeat(64),
            created_at: "now".into(),
        };
        for (server, ack) in [
            ("https://site.example.org", false),
            ("https://other.example.org", true),
        ] {
            assert!(state
                .apply(Change::ShareNsiteAsset {
                    id: id.clone(),
                    server: server.into(),
                    html: html.clone(),
                    receipt: receipt.clone(),
                    acknowledge_public: ack
                })
                .is_err());
        }
        state
            .apply(Change::ShareNsiteAsset {
                id: id.clone(),
                server: "https://site.example.org".into(),
                html: html.clone(),
                receipt,
                acknowledge_public: true,
            })
            .unwrap();
        // Persisted snapshots keep the exact selection; a later draft cannot alter it.
        let mut state: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        state.projects.get_mut(&id).unwrap().draft = "private later draft".into();
        let req = Request::builder()
            .uri(format!("/{hash}"))
            .body(Body::empty())
            .unwrap();
        let response = response_for(&state, &id, port, Route::Fips, &req);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["access-control-allow-origin"], "*");
        assert_eq!(
            hyper::body::to_bytes(response.into_body()).await.unwrap(),
            html
        );
        for path in ["/list", "/upload", "/rpc", "/other-hash"] {
            let req = Request::builder().uri(path).body(Body::empty()).unwrap();
            assert_eq!(
                response_for(&state, &id, port, Route::Fips, &req).status(),
                StatusCode::NOT_FOUND
            );
        }
        state
            .apply(Change::UnshareNsiteAsset { id: id.clone() })
            .unwrap();
        assert_eq!(
            response_for(&state, &id, port, Route::Fips, &req).status(),
            StatusCode::NOT_FOUND
        );
        let root = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert_eq!(
            response_for(&state, &id, port, Route::Fips, &root).status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn public_archive_is_exact_explicit_route_scoped_and_revocable() {
        use crate::publishing::{Change, LocalArchive, Route};
        let mut state = State::default();
        let id = state
            .apply(Change::Create {
                name: "Archive".into(),
            })
            .unwrap()
            .unwrap();
        state
            .apply(Change::Save {
                id: id.clone(),
                name: "Archive".into(),
                routes: [Route::Fips, Route::Tor].into_iter().collect(),
                domain: None,
                html: "public snapshot".into(),
            })
            .unwrap();
        state
            .apply(Change::PublishFips {
                id: id.clone(),
                acknowledge_public: true,
            })
            .unwrap();
        state
            .apply(Change::PublishTor {
                id: id.clone(),
                acknowledge_public: true,
            })
            .unwrap();
        let port = state.projects[&id].fips_publication.as_ref().unwrap().port;
        let tor_port = state.projects[&id].tor_publication.as_ref().unwrap().port;
        let hash = crate::publishing::nsite::hash(b"public snapshot");
        let req = Request::builder()
            .uri(format!("/{hash}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            response(&state, &id, port, &req).status(),
            StatusCode::NOT_FOUND
        );
        assert!(state
            .apply(Change::ShareArchive {
                id: id.clone(),
                route: Route::Fips,
                acknowledge_public: true
            })
            .is_err());
        state
            .apply(Change::RecordLocalArchive {
                id: id.clone(),
                receipt: LocalArchive {
                    sha256: hash.clone(),
                    size: 15,
                    pubkey: "a".repeat(64),
                    created_at: "now".into(),
                },
            })
            .unwrap();
        assert!(state
            .apply(Change::ShareArchive {
                id: id.clone(),
                route: Route::Fips,
                acknowledge_public: false
            })
            .is_err());
        state
            .apply(Change::ShareArchive {
                id: id.clone(),
                route: Route::Fips,
                acknowledge_public: true,
            })
            .unwrap();
        assert_eq!(
            response_for(&state, &id, tor_port, Route::Tor, &req).status(),
            StatusCode::NOT_FOUND
        );
        let r = response(&state, &id, port, &req);
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()["access-control-allow-origin"], "*");
        assert!(r.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment"));
        assert_eq!(r.headers()["content-security-policy"], CSP);
        assert_eq!(
            hyper::body::to_bytes(r.into_body()).await.unwrap().as_ref(),
            b"public snapshot"
        );
        for path in [
            "/upload",
            "/list",
            "/0000000000000000000000000000000000000000000000000000000000000000",
            "/../state.json",
        ] {
            let r = Request::builder().uri(path).body(Body::empty()).unwrap();
            assert_eq!(
                response(&state, &id, port, &r).status(),
                StatusCode::NOT_FOUND
            );
        }
        let head = Request::builder()
            .method(Method::HEAD)
            .uri(format!("/{hash}"))
            .body(Body::empty())
            .unwrap();
        let r = response(&state, &id, port, &head);
        assert_eq!(r.headers()["content-length"], "15");
        assert!(hyper::body::to_bytes(r.into_body())
            .await
            .unwrap()
            .is_empty());
        let post = Request::builder()
            .method(Method::PUT)
            .uri(format!("/{hash}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            response(&state, &id, port, &post).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        state.projects.get_mut(&id).unwrap().draft = "private later edits".into();
        assert_eq!(
            hyper::body::to_bytes(response(&state, &id, port, &req).into_body())
                .await
                .unwrap()
                .as_ref(),
            b"public snapshot"
        );
        state
            .apply(Change::UnshareArchive {
                id: id.clone(),
                route: Route::Fips,
            })
            .unwrap();
        assert_eq!(
            response(&state, &id, port, &req).status(),
            StatusCode::NOT_FOUND
        );
        state
            .apply(Change::ShareArchive {
                id: id.clone(),
                route: Route::Fips,
                acknowledge_public: true,
            })
            .unwrap();
        state
            .apply(Change::PublishFips {
                id: id.clone(),
                acknowledge_public: true,
            })
            .unwrap();
        assert_eq!(
            response(&state, &id, port, &req).status(),
            StatusCode::NOT_FOUND
        );
        assert!(state
            .apply(Change::ShareArchive {
                id,
                route: Route::Fips,
                acknowledge_public: true
            })
            .is_err());
    }

    #[tokio::test]
    async fn draft_changes_never_leak_and_unpublish_revokes() {
        use crate::publishing::{Change, Route};
        let mut state = State::default();
        let id = state
            .apply(Change::Create {
                name: "Example".into(),
            })
            .unwrap()
            .unwrap();
        state
            .apply(Change::Save {
                id: id.clone(),
                name: "Example".into(),
                routes: [Route::Fips, Route::Tor].into_iter().collect(),
                domain: None,
                html: "old".into(),
            })
            .unwrap();
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
        let port = state.projects[&id].fips_publication.as_ref().unwrap().port;
        assert!(state
            .apply(Change::PublishTor {
                id: id.clone(),
                acknowledge_public: false
            })
            .is_err());
        state
            .apply(Change::PublishTor {
                id: id.clone(),
                acknowledge_public: true,
            })
            .unwrap();
        let tor_port = state.projects[&id].tor_publication.as_ref().unwrap().port;
        assert_ne!(port, tor_port);
        state.projects.get_mut(&id).unwrap().draft = "unpublished secret draft".into();
        let req = Request::builder()
            .uri("/")
            .header("cookie", "session=secret")
            .body(Body::empty())
            .unwrap();
        let r = response(&state, &id, port, &req);
        assert_eq!(r.headers()["content-security-policy"], CSP);
        assert!(!r.headers().contains_key("set-cookie"));
        assert_eq!(
            hyper::body::to_bytes(r.into_body()).await.unwrap().as_ref(),
            b"old"
        );
        for path in ["/rpc", "/../state.json", "/index.html/other"] {
            let req = Request::builder().uri(path).body(Body::empty()).unwrap();
            assert_eq!(
                response(&state, &id, port, &req).status(),
                StatusCode::NOT_FOUND
            );
        }
        state
            .apply(Change::UnpublishFips { id: id.clone() })
            .unwrap();
        assert_eq!(
            response(&state, &id, port, &req).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response_for(&state, &id, tor_port, Route::Tor, &req).status(),
            StatusCode::OK
        );
        state
            .apply(Change::UnpublishTor { id: id.clone() })
            .unwrap();
        assert_eq!(
            response_for(&state, &id, tor_port, Route::Tor, &req).status(),
            StatusCode::NOT_FOUND
        );
    }
}
