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
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return simple(
            StatusCode::METHOD_NOT_ALLOWED,
            "Only GET and HEAD are supported",
        );
    }
    if !matches!(req.uri().path(), "/" | "/index.html") {
        return simple(StatusCode::NOT_FOUND, "Not found");
    }
    let mut response = simple(StatusCode::OK, "");
    response
        .headers_mut()
        .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
    response.headers_mut().insert(
        "content-length",
        publication.html.len().to_string().parse().unwrap(),
    );
    if req.method() == Method::GET {
        *response.body_mut() = Body::from(publication.html.clone());
    }
    response
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
