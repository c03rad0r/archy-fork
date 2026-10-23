use anyhow::{Context, Result};
use tracing::info;

use crate::opkg::PkgManager;
use crate::tollgate::version::{
    decide_install, package_version_from_pin, parse_installed, Installed, TOLLGATE_PACKAGE,
    TOLLGATE_SERVICE_BIN,
};
use crate::Router;

/// Pinned upstream release. Was stuck on v0.2.0 (Oct 2025) until 2026-09-05 —
/// nine releases behind. v0.5.0's changelog covers exactly the failure modes
/// hit live against archy-x250-pa3: a mint with an empty/broken keyset used
/// to crash-loop the daemon forever ("graceful degradation when Cashu mints
/// fail" in v0.5.0), and the bundled captive-portal build had no CBOR support
/// at all, so it could only decode legacy `cashuA` tokens — rejecting the
/// `cashuB` (NUT-00 V4) tokens modern wallets like Minibits generate by
/// default ("portal improvements" in v0.5.0 include a JS bundle update that
/// should carry a current cashu-ts with V4 support). Bump this string to move
/// both this crate's URLs and the version baked into the source comments.
const TOLLGATE_VERSION: &str = "v0.5.0";

/// Direct-download fallback URLs by opkg architecture string, for the
/// `.ipk` (ar-archive) package format.
/// Used when the package is not in any configured feed.
/// Source: https://github.com/OpenTollGate/tollgate-module-basic-go/releases/tag/v0.5.0
fn ipk_url(arch: &str) -> Option<String> {
    let name = match arch {
        "mips_24kc" => "mips_24kc",
        "mipsel_24kc" => "mipsel_24kc",
        "aarch64_cortex-a53" => "aarch64_cortex-a53",
        "aarch64_cortex-a72" => "aarch64_cortex-a72",
        "arm_cortex-a7" => "arm_cortex-a7",
        "x86_64" => "x86_64",
        _ => return None,
    };
    Some(format!(
        "https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/{TOLLGATE_VERSION}/tollgate-wrt_{TOLLGATE_VERSION}_{name}.ipk"
    ))
}

/// Direct-download URLs for the native Alpine-style `.apk` package format —
/// only published for a subset of architectures as of v0.5.0. Where
/// available this is strictly better than [`ipk_url`] on an apk-native
/// (OpenWrt 25.x+) router: `apk add` installs it directly (dependency
/// resolution, postinst, uci-defaults all handled by apk itself), instead of
/// the manual `ar`/`tar` extraction dance `install_ipk` has to do to unpack
/// an `.ipk` on a router with no `opkg`.
fn apk_url(arch: &str) -> Option<String> {
    let name = match arch {
        "aarch64_cortex-a53" => "aarch64_cortex-a53",
        "x86_64" => "x86_64",
        _ => return None,
    };
    Some(format!(
        "https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/{TOLLGATE_VERSION}/tollgate-wrt_{TOLLGATE_VERSION}_{name}.apk"
    ))
}

/// Install tollgate-module-basic-go via opkg (OpenWrt ≤24.x).
///
/// Tries opkg first (works if a custom feed is configured). Falls back to
/// downloading the .ipk directly from GitHub releases if opkg can't find it.
/// Caller is responsible for running `opkg_update` first.
pub fn install_tollgate(router: &Router) -> Result<()> {
    info!("[{}] Installing {}", router.host, TOLLGATE_PACKAGE);

    // Fast path: standard opkg install (or already installed).
    if router.opkg_install(TOLLGATE_PACKAGE).is_ok() {
        return Ok(());
    }

    // Package not in any feed — download the .ipk directly.
    let arch = router.run_ok(
        "opkg print-architecture | grep -v all | grep -v noarch | tail -1 | awk '{print $2}'",
    )?;
    let arch = arch.trim();

    let url = ipk_url(arch).ok_or_else(|| {
        anyhow::anyhow!(
            "No pre-built TollGate package for architecture '{}'. \
             Add a custom opkg feed or build from source.",
            arch
        )
    })?;

    info!(
        "[{}] Downloading TollGate for {} from GitHub releases",
        router.host, arch
    );
    router.run_ok(&format!(
        "wget --no-check-certificate -O /tmp/tollgate.ipk '{}' 2>&1",
        url
    ))?;
    install_ipk(router, "/tmp/tollgate.ipk")
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

    // Get architecture from /etc/openwrt_release.
    // The variable is DISTRIB_ARCH on most builds; OPENWRT_ARCH on some.
    // Fall back to apk --print-arch, then uname -m.
    let arch_raw = router.run_ok(
        ". /etc/openwrt_release 2>/dev/null \
         && a=\"${DISTRIB_ARCH:-${OPENWRT_ARCH:-}}\" \
         && [ -n \"$a\" ] && echo \"$a\" \
         || apk --print-arch 2>/dev/null \
         || uname -m",
    )?;
    // Normalise: uname -m returns bare "mipsel"/"mips"; map to 24kc variant
    // which is the standard for home-router MIPS builds.
    let arch = match arch_raw.trim() {
        "mipsel" => "mipsel_24kc",
        "mips" => "mips_24kc",
        other => other,
    };
    info!("[{}] detected arch: {:?}", router.host, arch);
    if arch.is_empty() {
        anyhow::bail!("Could not determine router architecture");
    }

    // Prefer a native .apk when the release publishes one for this arch —
    // `apk add` handles the install itself (deps, postinst, uci-defaults),
    // skipping the manual ar/tar extraction the .ipk fallback below needs.
    if let Some(url) = apk_url(arch) {
        info!(
            "[{}] Downloading native TollGate .apk for {} from GitHub releases",
            router.host, arch
        );
        let (dl_out, dl_code) = router.run(&format!(
            "wget --no-check-certificate -O /tmp/tollgate.apk '{}' 2>&1",
            url
        ))?;
        if dl_code != 0 {
            anyhow::bail!("TollGate .apk download failed: {}", dl_out.trim());
        }
        let (size_out, _) = router.run("wc -c < /tmp/tollgate.apk 2>/dev/null")?;
        let size: u64 = size_out.trim().parse().unwrap_or(0);
        if size < 50_000 {
            anyhow::bail!(
                "Downloaded TollGate .apk is only {}B — wget likely captured an error page. \
                 Check router internet access and that the release URL is reachable.",
                size
            );
        }
        let (add_out, add_code) = router.run("apk add --allow-untrusted /tmp/tollgate.apk 2>&1")?;
        router.run_ok("rm -f /tmp/tollgate.apk")?;
        if add_code != 0 {
            anyhow::bail!("TollGate .apk install failed: {}", add_out.trim());
        }
        return Ok(());
    }

    let url = ipk_url(arch).ok_or_else(|| {
        anyhow::anyhow!(
            "No pre-built TollGate package for architecture '{}'. \
             Add a custom feed or build from source.",
            arch
        )
    })?;

    info!(
        "[{}] Downloading TollGate for {} from GitHub releases",
        router.host, arch
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
