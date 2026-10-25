use anyhow::{Context, Result};
use tracing::info;

use crate::opkg::PkgManager;
use crate::tollgate::assets::{asset_url, AssetShape, PackageTarget, FEED_REPO};
use crate::tollgate::verify::{manifest_url, verify_download, MIN_PACKAGE_BYTES, SHA256SUMS_NAME};
use crate::tollgate::version::{
    decide_install, package_version_from_pin, parse_installed, Installed, TOLLGATE_PACKAGE,
    TOLLGATE_SERVICE_BIN,
};
use crate::Router;

/// Pinned TollGate release.
///
/// **This is the feed release tag, not a tag on the reference-implementation
/// repository.** Up to and including v0.5.0 the packages were attached to a
/// release on `OpenTollGate/tollgate-module-basic-go`. From v0.6.0 they are
/// built and signed by the OpenWrt packages feed, and the same tag on the old
/// repository carries **zero assets** — so a bump that keeps the old
/// repository 404s on every architecture while pointing at a release tag that
/// genuinely exists. See [`crate::tollgate::assets`] for the full account.
///
/// Bump history:
/// * v0.2.0 (Oct 2025) → v0.5.0, 2026-09-05 — nine releases behind; v0.5.0
///   carried the Cashu-mint crash-loop fix and the CBOR-capable portal build
///   that could decode `cashuB` (NUT-00 V4) tokens.
/// * v0.5.0 → v0.6.0 — packages moved to the feed; asset names became
///   target-qualified and the filename version dropped its leading `v`.
///
/// Rolling this constant back to `"v0.5.0"` restores the previous download
/// path, provided the URL is built with [`AssetShape::LegacyArchOnly`] against
/// [`LEGACY_REPO`] — which is why the shape is chosen from the same place the
/// version is, rather than hardcoded at each call site.
pub const TOLLGATE_VERSION: &str = "v0.6.0";

/// The asset-name scheme that matches [`TOLLGATE_VERSION`].
///
/// Kept adjacent to the version on purpose: the two changed together, and
/// separating them is how a bump produces a URL that is well-formed and wrong.
const TOLLGATE_SHAPE: AssetShape = AssetShape::FeedTargetQualified;

/// The repository that publishes the assets matching [`TOLLGATE_VERSION`].
const TOLLGATE_REPO: &str = FEED_REPO;

/// Direct-download URL for the `.ipk` (ar-archive) format.
///
/// Used when the package is not in any configured feed. Requires the router's
/// OpenWrt target as well as its architecture — see
/// [`PackageTarget`](crate::tollgate::assets::PackageTarget).
fn ipk_url(target: &PackageTarget) -> Option<String> {
    if !target.is_complete() {
        return None;
    }
    Some(asset_url(
        TOLLGATE_REPO,
        TOLLGATE_VERSION,
        TOLLGATE_SHAPE,
        &target.arch,
        &target.target,
        "ipk",
    ))
}

/// Direct-download URL for the native Alpine-style `.apk` format.
///
/// Where available this is strictly better than [`ipk_url`] on an apk-native
/// (OpenWrt 25.x+) router: `apk add` installs it directly (dependency
/// resolution, postinst, uci-defaults all handled by apk itself), instead of
/// the manual `ar`/`tar` extraction dance [`install_ipk`] has to do to unpack
/// an `.ipk` on a router with no `opkg`.
///
/// The v0.5.0-era code published `.apk` for only two architectures, which made
/// every other apk-native router fall through to the manual path. From v0.6.0
/// both formats are published for all eight `(arch, target)` pairs, so the
/// allow-list is gone — an unsupported pair now fails on the fetch rather than
/// being silently excluded.
fn apk_url(target: &PackageTarget) -> Option<String> {
    if !target.is_complete() {
        return None;
    }
    Some(asset_url(
        TOLLGATE_REPO,
        TOLLGATE_VERSION,
        TOLLGATE_SHAPE,
        &target.arch,
        &target.target,
        "apk",
    ))
}

/// Install tollgate-module-basic-go via opkg (OpenWrt ≤24.x).
///
/// Tries opkg first (works if a custom feed is configured). Falls back to
/// downloading the .ipk directly from GitHub releases if opkg can't find it.
/// Caller is responsible for running `opkg_update` first.
pub fn install_tollgate(router: &Router) -> Result<()> {
    info!("[{}] Installing {}", router.host, TOLLGATE_PACKAGE);

    // The version decision comes FIRST, and it is the same decision the apk path
    // makes. It did not use to be here at all, and the omission was invisible in
    // a specific and dangerous way.
    //
    // `router.opkg_install()` is not version-aware: it greps
    // `opkg list-installed` for the package name and returns Ok(()) if it is
    // present, at ANY version. While the constant was the repository name
    // (`tollgate-module-basic-go`) that grep could never match, so control fell
    // through to a real install and the missing version check did not matter.
    // Renaming the constant to the real package name — required for the apk
    // guard — made the grep match again, which turned this path into the very
    // silent no-op this change exists to remove, on OpenWrt ≤24.x.
    //
    // So the guard is explicit here, not delegated to opkg_install.
    let installed = installed_tollgate_version(router, PkgManager::Opkg);
    let desired = package_version_from_pin(TOLLGATE_VERSION)
        .context("derive the package version from the pinned release tag")?;
    let decision = decide_install(&installed, &desired, false, None);
    if !decision.should_install() {
        info!("[{}] {}", router.host, decision.reason());
        return Ok(());
    }
    info!("[{}] {}", router.host, decision.reason());

    // Fast path: the package is in a configured feed. `opkg install` on an
    // ALREADY-INSTALLED package does not upgrade it, so the version decision
    // above had to run first — asking opkg to install only after we know the
    // installed version is wrong.
    if let Ok(()) = router.opkg_install_forced(TOLLGATE_PACKAGE) {
        return Ok(());
    }

    // Package not in any feed — download the .ipk directly.
    //
    // The architecture alone is no longer sufficient: from v0.6.0 the asset
    // name is target-qualified, because one CPU arch legitimately serves
    // several OpenWrt targets (`mips_24kc` is both `ath79-generic` and
    // `ath79-nand`). Read both halves.
    let target = read_package_target(router)?;

    let url = ipk_url(&target).ok_or_else(|| {
        anyhow::anyhow!(
            "Could not build a TollGate package URL for arch '{}' target '{}'. \
             Both values are required — an arch-only asset name no longer exists. \
             Add a custom opkg feed or build from source.",
            target.arch,
            target.target
        )
    })?;

    info!(
        "[{}] Downloading TollGate for {} / {} from the package feed",
        router.host, target.arch, target.target
    );
    router.run_ok(&format!(
        "wget --no-check-certificate -O /tmp/tollgate.ipk '{}' 2>&1",
        url
    ))?;
    verify_downloaded_package(router, "/tmp/tollgate.ipk", &filename_from_url(&url))?;
    install_ipk(router, "/tmp/tollgate.ipk")
}

/// Install the runtime dependencies the TollGate package declares.
///
/// The feed recipe declares `DEPENDS:=+nodogsplash +jq +ca-bundle`. None of
/// them arrive on their own here, because every install path in this module
/// deliberately skips dependency resolution:
///
/// * the `opkg` fast path only resolves deps against a real feed, and there is
///   usually no TollGate feed configured;
/// * the raw `.ipk` path extracts with `ar`/`tar` and runs the scriptlets
///   directly — no solver at all;
/// * the `.apk` path uses `--allow-untrusted` on a local file.
///
/// `nodogsplash` is installed separately (it is also stopped and reconfigured,
/// see `nodogsplash.rs`), so only the other two are handled here:
///
/// * **`jq`** — load-bearing. `99-tollgate-setup` reads `entry_ui` out of
///   `config.json` with it, and falls back to the `board` mapping when jq is
///   absent. A missing jq therefore silently changes which UI answers the
///   router's entry ports.
/// * **`ca-bundle`** — TLS trust. The gateway verifies Cashu mints and
///   Lightning over TLS using the system root pool only, so without it a
///   router cannot verify a payment.
///
/// Both are Architecture: all and present in both the 24.10 opkg and 25.12 apk
/// indexes, so a plain install resolves either way.
pub fn install_runtime_deps(router: &Router, pkg_mgr: PkgManager) -> Result<()> {
    for dep in ["jq", "ca-bundle"] {
        router
            .install_package(pkg_mgr, dep)
            .with_context(|| format!("install TollGate runtime dependency {dep}"))?;
    }
    Ok(())
}

/// The basename of a URL's last path segment.
///
/// Used to name the file in the manifest lookup, so the digest is checked
/// against the name the release actually published.
fn filename_from_url(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).to_string()
}

/// Fetch a release's `SHA256SUMS` and verify a downloaded file against it.
///
/// Runs on the ROUTER, not here: the file being checked lives there, and
/// hashing it locally would mean trusting a second transfer.
///
/// Fails closed. An unreachable manifest, an absent entry, or a digest
/// mismatch all abort the install — this is a money path, and a package whose
/// integrity cannot be checked is not installable.
pub fn verify_downloaded_package(router: &Router, path: &str, filename: &str) -> Result<()> {
    let (size_out, _) = router.run(&format!("wc -c < {path} 2>/dev/null"))?;
    let bytes: u64 = size_out.trim().parse().unwrap_or(0);

    // Cheap pre-check first: a tiny file is an error page, and saying so is
    // more useful than a digest mismatch on HTML.
    if bytes < MIN_PACKAGE_BYTES {
        anyhow::bail!("{}", verify_download(filename, bytes, "", "").message());
    }

    // The release manifest. `--no-check-certificate` for the same reason the
    // package download uses it: fresh OpenWrt images ship without a CA bundle,
    // which is one of the reasons we install ca-bundle below.
    let manifest_url = manifest_url(TOLLGATE_REPO, TOLLGATE_VERSION);
    let (_, dl_code) = router.run(&format!(
        "wget --no-check-certificate -q -O /tmp/tollgate-SHA256SUMS '{manifest_url}' 2>&1"
    ))?;
    if dl_code != 0 {
        anyhow::bail!(
            "could not fetch {SHA256SUMS_NAME} from {manifest_url} — refusing to install a \
             package whose integrity cannot be verified against the release manifest"
        );
    }
    let (manifest_text, _) = router.run("cat /tmp/tollgate-SHA256SUMS 2>/dev/null")?;

    let (digest_out, digest_code) = router.run(&format!(
        // The exit status of a pipeline is its LAST command's, so this used to
        // end in `| awk` and `digest_code` was always 0 — the "sha256sum is not
        // available" branch below was dead, and a router without it produced a
        // confusing MISMATCH with an empty actual digest instead. `test -x` first
        // makes the guard mean what it says.
        "command -v sha256sum >/dev/null 2>&1 || exit 3; sha256sum {path} 2>/dev/null | awk '{{print $1}}'"
    ))?;
    if digest_code != 0 {
        anyhow::bail!("sha256sum is not available on this router — cannot verify the download");
    }
    let actual = digest_out.trim();

    let outcome = verify_download(filename, bytes, actual, &manifest_text);
    router.run_ok("rm -f /tmp/tollgate-SHA256SUMS")?;

    if !outcome.may_install() {
        anyhow::bail!("{}", outcome.message());
    }
    info!("[{}] {}", router.host, outcome.message());
    Ok(())
}

/// Read `DISTRIB_ARCH` and `DISTRIB_TARGET` from the router.
///
/// Falls back to the package manager's own architecture report and then
/// `uname -m` for the architecture, because `DISTRIB_ARCH` is absent on some
/// builds. There is deliberately **no fallback for the target**: a guessed
/// target builds a well-formed URL for the wrong package, which is worse than
/// a loud failure.
pub fn read_package_target(router: &Router) -> Result<PackageTarget> {
    let (out, _) = router.run(PackageTarget::READ_CMD)?;
    let mut target = PackageTarget::parse(&out);

    if target.arch.is_empty() {
        let (fallback, _) = router.run(
            "apk --print-arch 2>/dev/null || opkg print-architecture 2>/dev/null \
             | grep -v all | grep -v noarch | tail -1 | awk '{print $2}' || uname -m",
        )?;
        // `uname -m` returns bare "mipsel"/"mips"; map to the 24kc variant,
        // which is the standard for home-router MIPS builds.
        target.arch = match fallback.trim() {
            "mipsel" => "mipsel_24kc".to_string(),
            "mips" => "mips_24kc".to_string(),
            other => other.to_string(),
        };
    }

    Ok(target)
}

/// Install tollgate-module-basic-go on OpenWrt 25.x where opkg is not available.
///
/// Downloads the .ipk from GitHub releases and extracts it manually using
/// BusyBox `ar` and `tar` (both present on all OpenWrt images).
pub fn install_tollgate_apk_native(router: &Router) -> Result<()> {
    info!(
        "[{}] Installing {} (apk-native mode)",
        router.host, TOLLGATE_PACKAGE
    );

    // Already installed AT THE RIGHT VERSION?
    //
    // This guard used to be:
    //     apk list --installed | grep -q '^tollgate-module-basic-go' \
    //       || test -f /usr/bin/tollgate-wrt
    //
    // Both halves were wrong and the failure was silent:
    //   * the package is named `tollgate-wrt` (PKG_NAME), never
    //     `tollgate-module-basic-go` (that is the repository name), so the grep
    //     could never match;
    //   * the file-existence fallback then succeeded on any router that had
    //     ever been provisioned, so the function returned Ok(()) without
    //     installing.
    // Net effect: bumping the pinned version changed nothing on an
    // already-provisioned router, while the UI reported success.
    //
    // The replacement compares VERSIONS. A router on 0.5.0 with a 0.6.0 pin
    // must install; only a genuine version match skips.
    let installed = installed_tollgate_version(router, PkgManager::ApkNative);
    let desired = package_version_from_pin(TOLLGATE_VERSION)
        .context("derive the package version from the pinned release tag")?;
    let decision = decide_install(&installed, &desired, false, None);
    if !decision.should_install() {
        info!("[{}] {}", router.host, decision.reason());
        return Ok(());
    }
    info!("[{}] {}", router.host, decision.reason());

    // Both halves of the package identity, read from the router.
    let target = read_package_target(router)?;
    if !target.is_complete() {
        anyhow::bail!(
            "Could not determine this router's OpenWrt target (arch '{}', target '{}'). \
             From v0.6.0 the TollGate package name is target-qualified, so the architecture \
             alone is not enough to build a download URL. Check that /etc/openwrt_release \
             defines DISTRIB_TARGET.",
            target.arch,
            target.target
        );
    }
    info!(
        "[{}] detected arch: {:?} target: {:?}",
        router.host, target.arch, target.target
    );

    // Prefer a native .apk — `apk add` handles the install itself (deps,
    // postinst, uci-defaults), skipping the manual ar/tar extraction the .ipk
    // fallback below needs.
    if let Some(url) = apk_url(&target) {
        info!(
            "[{}] Downloading native TollGate .apk for {} / {} from the package feed",
            router.host, target.arch, target.target
        );
        let (dl_out, dl_code) = router.run(&format!(
            "wget --no-check-certificate -O /tmp/tollgate.apk '{}' 2>&1",
            url
        ))?;
        if dl_code != 0 {
            anyhow::bail!("TollGate .apk download failed: {}", dl_out.trim());
        }
        verify_downloaded_package(router, "/tmp/tollgate.apk", &filename_from_url(&url))?;
        let (add_out, add_code) = router.run("apk add --allow-untrusted /tmp/tollgate.apk 2>&1")?;
        router.run_ok("rm -f /tmp/tollgate.apk")?;
        if add_code != 0 {
            anyhow::bail!("TollGate .apk install failed: {}", add_out.trim());
        }
        return Ok(());
    }

    let url = ipk_url(&target).ok_or_else(|| {
        anyhow::anyhow!(
            "Could not build a TollGate package URL for arch '{}' target '{}'. \
             Add a custom feed or build from source.",
            target.arch,
            target.target
        )
    })?;

    info!(
        "[{}] Downloading TollGate for {} / {} from the package feed",
        router.host, target.arch, target.target
    );
    // --no-check-certificate: fresh OpenWrt 25.x images ship without a CA bundle;
    // GitHub serves releases over HTTPS so wget would otherwise reject the cert.
    let (dl_out, dl_code) = router.run(&format!(
        "wget --no-check-certificate -O /tmp/tollgate.ipk '{}' 2>&1",
        url
    ))?;
    if dl_code != 0 {
        anyhow::bail!("TollGate download failed: {}", dl_out.trim());
    }
    // Sanity-check: a real .ipk is at least 50 KB.
    // If wget captured an HTML error page it will be tiny.
    let (size_out, _) = router.run("wc -c < /tmp/tollgate.ipk 2>/dev/null")?;
    let size: u64 = size_out.trim().parse().unwrap_or(0);
    if size < 50_000 {
        anyhow::bail!(
            "Downloaded TollGate package is only {}B — wget likely captured an error page. \
             Check router internet access and that the release URL is reachable.",
            size
        );
    }
    install_ipk(router, "/tmp/tollgate.ipk")
}

/// Read the currently-installed TollGate version from a router.
///
/// Two independent sources, because neither is sufficient alone:
///
/// * the package manager's installed list — authoritative, but absent when the
///   package was unpacked manually (`install_ipk`'s `ar`/`tar` path leaves no
///   record, which is the normal case on an apk-native router);
/// * the service binary's own `tollgate version` output — present after a
///   manual extraction, but it reports the *module build*, which is not the
///   same string as the feed's package version.
///
/// Returning `Installed::UnknownVersion` rather than guessing is deliberate:
/// an unreadable version must provoke an install attempt. Assuming it matches
/// is the defect that made a version bump a silent no-op.
pub fn installed_tollgate_version(router: &Router, pkg_mgr: PkgManager) -> Installed {
    let listing_cmd = match pkg_mgr {
        PkgManager::Opkg => "opkg list-installed 2>/dev/null",
        PkgManager::ApkNative => "apk list --installed 2>/dev/null",
    };
    let listing = router.run_ok(listing_cmd).unwrap_or_default();
    let from_listing = parse_installed(&listing, TOLLGATE_PACKAGE);
    if matches!(from_listing, Installed::Version(_)) {
        return from_listing;
    }

    // No usable package record — fall back to the binary's self-report.
    let (_, bin_present) = router
        .run(&format!("test -f {TOLLGATE_SERVICE_BIN} 2>/dev/null"))
        .unwrap_or((String::new(), 1));
    if bin_present != 0 {
        return Installed::Absent;
    }

    let (version_out, version_code) = router
        .run(&format!("{TOLLGATE_SERVICE_BIN} version 2>/dev/null"))
        .unwrap_or((String::new(), 1));
    if version_code == 0 {
        // `tollgate version` prints lines like `version: 0.5.0-gabc1234`.
        if let Some(v) = extract_version_line(&version_out) {
            return Installed::Version(v);
        }
    }

    Installed::UnknownVersion
}

/// Pull the version out of `tollgate version` output.
///
/// Kept separate and pure so it is unit-tested: the binary reports the
/// *module* build (`0.5.0-gabc1234`), not the feed's package version, so the
/// parsing has to tolerate a git-suffix and a `version:` label.
pub fn extract_version_line(output: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        let value = match line.split_once(':') {
            Some((label, rest)) if label.trim().eq_ignore_ascii_case("version") => rest.trim(),
            _ => continue,
        };
        // `0.5.0-gabc1234` -> `0.5.0`
        let without_git = value
            .split_once("-g")
            .map(|(head, _)| head)
            .unwrap_or(value);
        if !without_git.is_empty() {
            return Some(without_git.to_string());
        }
    }
    None
}

/// Extract and install an .ipk file without opkg.
///
/// An .ipk is an `ar` archive containing `data.tar.gz` (package files) and
/// `control.tar.gz` (metadata + postinst script).
fn install_ipk(router: &Router, ipk_path: &str) -> Result<()> {
    // Check for disk space first (rough: need at least ~1 MB free on /overlay).
    // TollGate is a Go binary — typically 5–8 MB on flash.
    let (df_out, _) = router.run("df /overlay 2>/dev/null | awk 'NR==2{print $4}'")?;
    let free_kb: u64 = df_out.trim().parse().unwrap_or(u64::MAX);
    if free_kb < 5120 {
        anyhow::bail!(
            "Not enough flash space for TollGate: only {}kB free on /overlay \
             (need ≥5MB). Free up space first or use a router with more storage.",
            free_kb
        );
    }

    router.run_ok("rm -rf /tmp/_tg_install && mkdir -p /tmp/_tg_install")?;

    // OpenWrt 25.x BusyBox does not include `ar` — install binutils via
    // whichever package manager is available before trying to unpack the ipk.
    let (_, ar_found) = router.run("command -v ar >/dev/null 2>&1")?;
    if ar_found != 0 {
        info!("[{}] ar not found, installing binutils", router.host);
        let (pkg_out, pkg_code) =
            router.run("apk add binutils 2>&1 || opkg install binutils 2>&1")?;
        if pkg_code != 0 {
            anyhow::bail!(
                "TollGate installation failed: ar not available and binutils install failed: {}",
                pkg_out.trim()
            );
        }
    }

    // Try standard opkg ar format first (ar archive → data.tar.gz inside).
    let (ar_out, ar_code) =
        router.run(&format!("cd /tmp/_tg_install && ar x {} 2>&1", ipk_path))?;

    if ar_code != 0 {
        // Fallback: some builds produce the .ipk as a gzip tarball rather than
        // a classic `ar` archive. This can still contain the same three ipk
        // members (debian-binary/data.tar.gz/control.tar.gz) one level deep —
        // just gzip-tarred together instead of ar'd — or, less commonly, a
        // flat tarball of the real package files with no ipk structure at
        // all. Extract to the scratch dir and check which shape it is before
        // deciding how to install it.
        info!(
            "[{}] ar failed ({}), trying tar -xzf",
            router.host,
            ar_out.trim()
        );

        // List contents first — validates format without writing anything.
        let (list_out, list_code) =
            router.run(&format!("tar -tzf {} 2>&1 | head -30", ipk_path))?;
        if list_code != 0 {
            anyhow::bail!(
                "TollGate installation failed: file is not an ar archive or gzip tar.\n\
                 ar: {}\ntar -t: {}",
                ar_out.trim(),
                list_out.trim()
            );
        }
        info!("[{}] ipk contents:\n{}", router.host, list_out.trim());

        router.run_ok(&format!("tar -xzf {} -C /tmp/_tg_install 2>&1", ipk_path))?;

        let (_, nested) = router.run("test -f /tmp/_tg_install/data.tar.gz")?;
        if nested != 0 {
            // Genuinely flat tarball, no ipk structure — its contents are the
            // real package files, already unpacked into the scratch dir.
            let (ov_df, _) = router.run("df / 2>/dev/null | awk 'NR==2{print $4}'")?;
            let overlay_free_kb: u64 = ov_df.trim().parse().unwrap_or(0);
            if overlay_free_kb < 5120 {
                anyhow::bail!(
                    "Not enough space to install TollGate: only {}kB free on /. \
                     Need at least 5MB. Free up flash space on the router first \
                     (e.g. remove unused packages with `apk del …`).",
                    overlay_free_kb
                );
            }
            let (cp_out, cp_code) = router.run("cp -a /tmp/_tg_install/. / 2>&1")?;
            if cp_code != 0 {
                anyhow::bail!(
                    "TollGate installation failed: file copy failed: {}",
                    cp_out.trim()
                );
            }
            // No package-manager postinst ran for these files either — see
            // the uci-defaults note below.
            router.run_ok(
                "for f in /etc/uci-defaults/*; do \
                     [ -f \"$f\" ] && ( cd \"$(dirname \"$f\")\" && . \"$f\" ) && rm -f \"$f\"; \
                 done; uci commit 2>/dev/null; true",
            )?;
            router.run_ok(&format!("rm -rf /tmp/_tg_install {}", ipk_path))?;
            return Ok(());
        }
        // Nested ipk-member layout — fall through to the shared unpack below.
    }

    // Unpack data.tar.gz (the real payload) from either the `ar`-extracted or
    // gzip-tar-extracted scratch dir, then run control.tar.gz's postinst.
    let (tar_out, tar_code) = router.run("tar -xzf /tmp/_tg_install/data.tar.gz -C / 2>&1")?;
    if tar_code != 0 {
        anyhow::bail!(
            "TollGate installation failed: data extract failed: {}",
            tar_out.trim()
        );
    }
    // Run postinst if present (optional — failures are non-fatal).
    router.run_ok(
        "if tar -xzf /tmp/_tg_install/control.tar.gz -C /tmp/_tg_install 2>/dev/null; then \
             chmod +x /tmp/_tg_install/postinst 2>/dev/null; \
             /tmp/_tg_install/postinst configure 2>/dev/null || true; \
         fi",
    )?;
    // `default_postinst` (what most packages' postinst calls, including
    // this one) only runs pending /etc/uci-defaults/* scripts for packages
    // it finds in opkg/apk's own file-list records. Since these files were
    // extracted manually rather than through a real package-manager install,
    // no such record exists, so run any pending scripts directly — this is
    // exactly what opkg's install path (or the next reboot) would otherwise
    // do for them, just without waiting for either.
    router.run_ok(
        "for f in /etc/uci-defaults/*; do \
             [ -f \"$f\" ] && ( cd \"$(dirname \"$f\")\" && . \"$f\" ) && rm -f \"$f\"; \
         done; uci commit 2>/dev/null; true",
    )?;

    router.run_ok(&format!("rm -rf /tmp/_tg_install {}", ipk_path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::extract_version_line;

    #[test]
    fn extracts_the_version_from_the_binarys_self_report() {
        let out = "TollGate Gateway\nversion: 0.5.0-gabc1234\ncommit: abc1234\n";
        assert_eq!(extract_version_line(out).as_deref(), Some("0.5.0"));
    }

    #[test]
    fn tolerates_a_plain_version_line() {
        let out = "version: 0.6.0\n";
        assert_eq!(extract_version_line(out).as_deref(), Some("0.6.0"));
    }

    #[test]
    fn is_not_fooled_by_a_commit_line_that_contains_the_word_version() {
        // Only a `version:` label counts; `commit:` must not be picked up.
        let out = "commit: deadbeef\nversion: 0.6.0-rc6-g1234abc\n";
        assert_eq!(extract_version_line(out).as_deref(), Some("0.6.0-rc6"));
    }

    #[test]
    fn returns_none_when_there_is_no_version_line() {
        // The caller turns this into UnknownVersion, which forces an install —
        // it must not be silently treated as a match.
        assert_eq!(extract_version_line("something else entirely\n"), None);
    }
}
