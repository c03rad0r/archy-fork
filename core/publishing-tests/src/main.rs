//! Explicit live smoke-test driver for the production publisher. Never starts
//! the backend, container reconciler or wallet services. Not a release artifact.
use anyhow::{bail, Context, Result};
use archipelago_publishing_tests::publishing::{self, Change, Route, Update};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .context("Usage: driver ROOT seed|run|unpublish ID")?,
    );
    let action = args.next().context("Missing action")?;
    // Deliberately cannot target installed application data.
    if !root.is_absolute() || root.file_name().and_then(|s| s.to_str()) != Some("publishing-smoke")
    {
        bail!("Use an absolute, dedicated publishing-smoke directory");
    }
    match action.as_str() {
        "seed" => {
            let mut state = publishing::load(&root).await?;
            if !state.projects.is_empty() {
                bail!("Smoke directory already contains projects");
            }
            for name in ["first", "second"] {
                let (s, id) = publishing::update(
                    &root,
                    Update {
                        version: state.version,
                        change: Change::Create { name: name.into() },
                    },
                )
                .await?;
                let id = id.unwrap();
                let (s, _) = publishing::update(&root, Update {
                    version: s.version, change: Change::Save {
                        id: id.clone(), name: name.into(), routes: [Route::Fips, Route::Tor].into_iter().collect(), domain: None,
                        html: format!("<!doctype html><title>Archipelago publishing check</title><h1>{name} website</h1>"),
                    },
                }).await?;
                let (s, _) = publishing::update(
                    &root,
                    Update {
                        version: s.version,
                        change: Change::PublishFips {
                            id: id.clone(),
                            acknowledge_public: true,
                        },
                    },
                )
                .await?;
                println!(
                    "{name} {id} {}",
                    s.projects[&id].fips_publication.as_ref().unwrap().port
                );
                state = s;
            }
        }
        "publish-tor" | "unpublish-tor" | "unpublish" => {
            let id = args.next().context("Missing project ID")?;
            let state = publishing::load(&root).await?;
            publishing::update(
                &root,
                Update {
                    version: state.version,
                    change: match action.as_str() {
                        "publish-tor" => Change::PublishTor {
                            id,
                            acknowledge_public: true,
                        },
                        "unpublish-tor" => Change::UnpublishTor { id },
                        _ => Change::UnpublishFips { id },
                    },
                },
            )
            .await?;
        }
        "run" => {
            let (stop, receive) = tokio::sync::watch::channel(false);
            let tor = tokio::spawn(publishing::tor::run(root.clone(), receive.clone()));
            let runner = tokio::spawn(publishing::serving::run(root, receive));
            tokio::signal::ctrl_c().await?;
            stop.send(true)?;
            runner.await?;
            tor.await?;
        }
        _ => bail!("Unknown action"),
    }
    Ok(())
}
