use anyhow::Result;
use tracing::info;

use crate::Router;

/// The OpenWrt package name for the TollGate reference implementation.
const TOLLGATE_PACKAGE: &str = "tollgate-module-basic-go";

/// Direct-download fallback URLs by opkg architecture string.
/// Used when the package is not in any configured feed.
/// Source: https://github.com/OpenTollGate/tollgate-module-basic-go/releases/tag/v0.2.0
fn ipk_url(arch: &str) -> Option<&'static str> {
    match arch {
        "mips_24kc"          => Some("https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/v0.2.0/mips_24kc.ipk"),
        "mipsel_24kc"        => Some("https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/v0.2.0/mipsel_24kc.ipk"),
        "aarch64_cortex-a53" => Some("https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/v0.2.0/aarch64_cortex-a53.ipk"),
        "aarch64_cortex-a72" => Some("https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/v0.2.0/aarch64_cortex-a72.ipk"),
        "arm_cortex-a7"      => Some("https://github.com/OpenTollGate/tollgate-module-basic-go/releases/download/v0.2.0/arm_cortex-a7.ipk"),
        _ => None,
    }
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
    let arch = router
        .run_ok("/usr/bin/opkg print-architecture | grep -v all | grep -v noarch | tail -1 | awk '{print $2}'")?;
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

    // Already installed? The service binary is /usr/bin/tollgate-wrt (per its
    // init.d script) — TOLLGATE_PACKAGE is only the opkg/apk package name,
    // never an on-disk filename, so it can't be used for the file-existence
    // fallback below.
    let (_, code) = router.run(&format!(
        "apk list --installed 2>/dev/null | grep -q '^{}' || \
         test -f /usr/bin/tollgate-wrt 2>/dev/null",
        TOLLGATE_PACKAGE
    ))?;
    if code == 0 {
        info!("[{}] {} already installed", router.host, TOLLGATE_PACKAGE);
        return Ok(());
    }

    // Get architecture from /etc/openwrt_release.
    // The variable is DISTRIB_ARCH on most builds; OPENWRT_ARCH on some.
    // Fall back to apk --print-arch, then uname -m.
    let arch_raw = router.run_ok(
        ". /etc/openwrt_release 2>/dev/null \
         && a=\"${DISTRIB_ARCH:-${OPENWRT_ARCH:-}}\" \
         && [ -n \"$a\" ] && echo \"$a\" \
         || /usr/bin/apk --print-arch 2>/dev/null \
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
