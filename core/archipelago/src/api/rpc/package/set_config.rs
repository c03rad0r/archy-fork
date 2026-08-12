//! Multi-version support — version listing + in-app version switch / pin /
//! auto-update toggle (`docs/bitcoin-multi-version-design.md` §3 Phase 3).
//!
//! Two RPCs:
//!   - `package.versions` — read the selectable versions for an app plus the
//!     runner's current pin / auto-update preference and (best-effort) the
//!     version actually running. Drives the install modal + "Version & Updates"
//!     card.
//!   - `package.set-config` — persist a version pin (or un-pin to track latest)
//!     and/or the auto-update toggle, then recreate the app at the chosen image
//!     when the version actually changed. A DOWNGRADE (older release over a
//!     newer chainstate — the highest-risk operation, design §4) is refused
//!     unless the caller passes `confirm: true`, so the UI can warn first.

use super::config::get_containers_for_app;
use super::install::install_log;
use super::validation::validate_app_id;
use crate::api::rpc::RpcHandler;
use crate::container::{app_catalog, version_config};
use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};

/// Apps that participate in multi-version selection today. Kept narrow on
/// purpose: version switching recreates the container, which is only safe for
/// the single-container, orchestrator-managed Bitcoin backends whose data and
/// downgrade semantics we understand. Any app the catalog gives a `versions[]`
/// list also qualifies (third-party registry apps inherit the capability).
fn supports_versions(app_id: &str) -> bool {
    matches!(app_id, "bitcoin-core" | "bitcoin-knots")
        || !app_catalog::catalog_versions(app_id).is_empty()
}

/// Extract the tag from a full image reference, leaving a `registry:port/repo`
/// host-port colon intact (only a colon AFTER the last `/` is a tag).
fn image_tag(image: &str) -> Option<String> {
    let after_slash = image.rsplit_once('/').map(|(_, r)| r).unwrap_or(image);
    after_slash
        .rsplit_once(':')
        .map(|(_, tag)| tag.to_string())
        .filter(|t| !t.is_empty())
}

/// Best-effort: the version tag of the backend container actually running for
/// `app_id`, by inspecting its image. `None` when not installed or unreadable.
async fn installed_version(app_id: &str) -> Option<String> {
    let containers = get_containers_for_app(app_id).await.ok()?;
    // Prefer the backend container (exact id / `archy-<id>`) over UI companions.
    //
    // The fallback is deliberately narrow. It used to be `containers.first()`
    // unconditionally, which for a multi-container stack reported a SIBLING's
    // version as the app's own: with btcpay-server's container absent, its
    // postgres dependency was first in the list, so package.versions answered
    // installedVersion "15.17" against an available "2.4.2". That is not a
    // cosmetic mislabel — it is the number the update decision is made from.
    //
    // A single-container app is unambiguous, so the fallback still covers apps
    // whose container is named differently from their id. With several
    // containers and no identifiable backend, "unknown" is the honest answer.
    let name = select_backend_container(app_id, &containers)?;
    let out = tokio::process::Command::new("podman")
        .args(["inspect", name, "--format", "{{.ImageName}}"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let image = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let tag = image_tag(&image)?;
    // A floating tag (latest/stable/...) names the reference used to CREATE the
    // container, not what's actually running — podman never re-resolves it once
    // cached, so a stale local `:latest` reports "latest" even when the real
    // `latest` moved on months ago (.228, 2026-07-01: ran a 4-month-old cached
    // image while a newer one already sat locally, unused). Ask the Bitcoin
    // backends directly instead of trusting the tag literal in that case.
    if is_floating_tag(&tag) {
        if let Some(real) = bitcoind_reported_version(app_id, name).await {
            return Some(real);
        }
    }
    Some(tag)
}

/// Pick the container that represents `app_id` itself, never a stack sibling.
///
/// See the note at the call site: an unconditional "first container" fallback
/// reported a dependency's image tag as the app's installed version.
fn select_backend_container<'a>(app_id: &str, containers: &'a [String]) -> Option<&'a str> {
    if let Some(exact) = containers
        .iter()
        .find(|n| n.as_str() == app_id || n.as_str() == format!("archy-{app_id}"))
    {
        return Some(exact.as_str());
    }
    // Unambiguous only when there is nothing else it could be.
    if containers.len() == 1 {
        return Some(containers[0].as_str());
    }
    None
}

fn is_floating_tag(tag: &str) -> bool {
    matches!(tag, "latest" | "stable" | "release" | "main")
}

/// Best-effort: ask the running bitcoind binary for its own version, trimmed to
/// the catalog's version-tag format (e.g. `29.3.knots20260210`, `29.2`). `None`
/// for apps other than the Bitcoin backends (no generic way to introspect a
/// third-party image's content version this way) or if the exec fails.
async fn bitcoind_reported_version(app_id: &str, container_name: &str) -> Option<String> {
    if !matches!(app_id, "bitcoin-core" | "bitcoin-knots") {
        return None;
    }
    let out = tokio::process::Command::new("podman")
        .args(["exec", container_name, "bitcoind", "--version"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_bitcoind_version_output(&String::from_utf8_lossy(&out.stdout))
}

/// Parses e.g. "Bitcoin Knots daemon version v29.3.knots20260210\n..." or
/// "Bitcoin Core version v29.2.0\n..." down to the version tag after `version v`.
fn parse_bitcoind_version_output(output: &str) -> Option<String> {
    let first_line = output.lines().next()?;
    let (_, version) = first_line.rsplit_once("version v")?;
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}

impl RpcHandler {
    /// `package.versions` — what a runner can install / switch to for this app,
    /// plus their current preference and the running version.
    pub(in crate::api::rpc) async fn handle_package_versions(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?;
        validate_app_id(app_id)?;

        let versions = app_catalog::catalog_versions(app_id);
        let default = app_catalog::catalog_default_version(app_id);
        let cfg = version_config::read(app_id);
        let installed = installed_version(app_id).await;

        Ok(serde_json::json!({
            "id": app_id,
            "supportsVersions": supports_versions(app_id),
            "default": default,
            "installedVersion": installed,
            "pinnedVersion": cfg.pinned_version,
            "autoUpdate": cfg.auto_update,
            "versions": versions.iter().map(|v| serde_json::json!({
                "version": v.version,
                "default": v.default,
                "deprecated": v.deprecated,
                "eol": v.eol,
            })).collect::<Vec<_>>(),
        }))
    }

    /// `package.set-config` — persist version pin + auto-update preference and
    /// recreate on an actual version change. Downgrades require `confirm:true`.
    pub(in crate::api::rpc) async fn handle_package_set_config(
        self: Arc<Self>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?
            .to_string();
        validate_app_id(&app_id)?;

        if !supports_versions(&app_id) {
            return Err(anyhow::anyhow!(
                "{} has no selectable versions in the catalog",
                app_id
            ));
        }

        let confirm = params
            .get("confirm")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let existing = version_config::read(&app_id);
        let default = app_catalog::catalog_default_version(&app_id);

        // ---- Resolve the requested pin (if a version was supplied) ----------
        // Absent `version` => leave the pin unchanged (an auto-update-only edit).
        // `version == default` => un-pin (track latest). Any other version must
        // exist in the catalog and resolve to a same-repo image, else reject.
        let version_param = params
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let mut new_pin = existing.pinned_version.clone();
        let mut version_changed = false;
        if let Some(req) = version_param.as_deref() {
            let resolved_pin = if default.as_deref() == Some(req) {
                None // selecting the default un-pins
            } else {
                // Validate the version is real + same-repo before pinning.
                if !app_catalog::catalog_versions(&app_id)
                    .iter()
                    .any(|v| v.version == req)
                {
                    return Err(anyhow::anyhow!(
                        "version {} is not offered for {}",
                        req,
                        app_id
                    ));
                }
                Some(req.to_string())
            };
            version_changed = resolved_pin != existing.pinned_version;
            new_pin = resolved_pin;
        }

        let new_auto_update = params
            .get("autoUpdate")
            .and_then(|v| v.as_bool())
            .unwrap_or(existing.auto_update);

        // ---- Downgrade gate (design §4: warn + confirm + allow) -------------
        // "Current" = what wrote the on-disk chainstate: the running version if
        // we can read it, else the existing pin, else the catalog default.
        if version_changed {
            let target = version_param.as_deref().unwrap_or_default();
            let current = installed_version(&app_id)
                .await
                .or_else(|| existing.pinned_version.clone())
                .or_else(|| default.clone());
            if let Some(current) = current {
                if version_config::is_downgrade(&current, target) && !confirm {
                    warn!(
                        "set-config {}: refusing un-confirmed downgrade {} -> {}",
                        app_id, current, target
                    );
                    return Ok(serde_json::json!({
                        "status": "confirm_required",
                        "kind": "downgrade",
                        "id": app_id,
                        "currentVersion": current,
                        "targetVersion": target,
                        "warning": format!(
                            "Switching {app_id} from {current} down to {target} is a \
                             downgrade. Bitcoin may refuse to start on a chainstate \
                             written by the newer version without a full reindex, and \
                             a pruned node can lose block data. Re-confirm to proceed."
                        ),
                    }));
                }
            }
        }

        // ---- Persist preference --------------------------------------------
        version_config::write(
            &app_id,
            &version_config::AppVersionConfig {
                pinned_version: new_pin.clone(),
                auto_update: new_auto_update,
            },
        )?;
        install_log(&format!(
            "SET-CONFIG {}: pinned={:?} autoUpdate={} (version_changed={})",
            app_id, new_pin, new_auto_update, version_changed
        ))
        .await;
        info!(
            app_id = %app_id,
            pinned = ?new_pin,
            auto_update = new_auto_update,
            version_changed,
            "package.set-config applied"
        );

        // ---- Recreate when the version actually changed + app is installed --
        // The orchestrator's install/recreate path reads the pin we just wrote
        // (prod_orchestrator image resolution), so reusing the update machinery
        // pulls + recreates at the chosen image. An auto-update-only edit, or a
        // change to a not-installed app, just persists the preference.
        let mut recreating = false;
        if version_changed {
            let installed = get_containers_for_app(&app_id)
                .await
                .map(|c| !c.is_empty())
                .unwrap_or(false);
            if installed {
                recreating = true;
                // Fire the existing async update flow; it flips state to
                // Updating and recreates honoring the new pin. The UI polls.
                self.clone()
                    .spawn_package_update(Some(serde_json::json!({ "id": app_id })))
                    .await?;
            }
        }

        Ok(serde_json::json!({
            "status": "ok",
            "id": app_id,
            "pinnedVersion": new_pin,
            "autoUpdate": new_auto_update,
            "versionChanged": version_changed,
            "recreating": recreating,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        image_tag, is_floating_tag, parse_bitcoind_version_output, select_backend_container,
    };

    #[test]
    fn floating_tag_detects_generic_channel_names() {
        for tag in ["latest", "stable", "release", "main"] {
            assert!(is_floating_tag(tag), "{tag}");
        }
        for tag in ["29.3.knots20260508", "28.4", "v29.2.0"] {
            assert!(!is_floating_tag(tag), "{tag}");
        }
    }

    #[test]
    fn parses_knots_version_line() {
        assert_eq!(
            parse_bitcoind_version_output(
                "Bitcoin Knots daemon version v29.3.knots20260210\nCopyright...\n"
            )
            .as_deref(),
            Some("29.3.knots20260210")
        );
    }

    #[test]
    fn parses_core_version_line() {
        assert_eq!(
            parse_bitcoind_version_output("Bitcoin Core version v29.2.0\n").as_deref(),
            Some("29.2.0")
        );
    }

    #[test]
    fn parse_returns_none_when_output_has_no_version_marker() {
        assert_eq!(parse_bitcoind_version_output("garbage output\n"), None);
        assert_eq!(parse_bitcoind_version_output(""), None);
    }

    /// The BTCPay case: with btcpay-server's own container absent, its postgres
    /// dependency was first in the app's container list and its tag (15.17) was
    /// reported as BTCPay's installed version, against an available 2.4.2.
    #[test]
    fn backend_selection_never_falls_back_to_a_sibling_in_a_stack() {
        let stack = vec!["archy-btcpay-db".to_string(), "archy-nbxplorer".to_string()];
        assert_eq!(select_backend_container("btcpay-server", &stack), None);

        let with_backend = vec!["archy-btcpay-db".to_string(), "btcpay-server".to_string()];
        assert_eq!(
            select_backend_container("btcpay-server", &with_backend),
            Some("btcpay-server")
        );
    }

    #[test]
    fn backend_selection_accepts_a_lone_differently_named_container() {
        let single = vec!["immich_server".to_string()];
        assert_eq!(
            select_backend_container("immich", &single),
            Some("immich_server")
        );
    }

    #[test]
    fn backend_selection_prefers_the_archy_prefixed_name() {
        let names = vec!["something-else".to_string(), "archy-nbxplorer".to_string()];
        assert_eq!(
            select_backend_container("nbxplorer", &names),
            Some("archy-nbxplorer")
        );
    }

    #[test]
    fn image_tag_keeps_registry_port_colon() {
        assert_eq!(
            image_tag("source.archipelago-foundation.org/lfg2025/bitcoin:28.4").as_deref(),
            Some("28.4")
        );
        assert_eq!(
            image_tag("source.archipelago-foundation.org/lfg2025/bitcoin-knots:29.3.knots20260508")
                .as_deref(),
            Some("29.3.knots20260508")
        );
        // No tag => None (don't mistake the registry port for a tag).
        assert_eq!(
            image_tag("source.archipelago-foundation.org/lfg2025/bitcoin"),
            None
        );
        assert_eq!(
            image_tag("docker.io/library/redis:7"),
            Some("7".to_string())
        );
    }
}
