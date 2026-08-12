use anyhow::Result;
use tracing::info;

use crate::Router;

/// Which package manager is available on this router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgManager {
    /// Traditional opkg (OpenWrt ≤24.x).
    Opkg,
    /// OpenWrt 25.x+ — apk is the native manager, opkg is not in repos.
    ApkNative,
}

impl Router {
    /// Detect which package manager is available.
    ///
    /// - If `/usr/bin/opkg` exists → `PkgManager::Opkg` (nothing to do).
    /// - If `/usr/bin/apk` exists → run `apk update` (switching repos to HTTP
    ///   first to work around missing CA bundle on fresh images), then try
    ///   `apk add opkg`.  If opkg is in the repos → `Opkg`.  If not (OpenWrt
    ///   25.x) → `ApkNative`.
    /// - Neither found → error.
    pub fn opkg_check(&self) -> Result<PkgManager> {
        let (_, code) = self.run("test -x /usr/bin/opkg")?;
        if code == 0 {
            return Ok(PkgManager::Opkg);
        }

        let (_, apk_code) = self.run("test -x /usr/bin/apk")?;
        if apk_code == 0 {
            info!("[{}] opkg not found — using apk (OpenWrt 25.x+)", self.host);
            // Fresh images ship without a CA bundle; switch repos to HTTP so
            // apk's wget can reach the package index without TLS verification.
            self.run_ok("sed -i 's|https://|http://|g' /etc/apk/repositories 2>/dev/null || true")?;
            let (update_out, update_code) = self.run("/usr/bin/apk update 2>&1")?;
            if update_code != 0 {
                anyhow::bail!(
                    "apk update failed (exit {}) — router may have no internet access. \
                     Ensure WAN/internet is working on the router before provisioning.\n{}",
                    update_code,
                    update_out.trim()
                );
            }
            // Try to install opkg (only available on some 25.x builds).
            let (add_out, add_code) = self.run("/usr/bin/apk add opkg 2>&1")?;
            if add_code == 0 {
                return Ok(PkgManager::Opkg);
            }
            if add_out.contains("no such package") || add_out.contains("unable to select") {
                info!(
                    "[{}] opkg not in apk repos — staying in apk-native mode",
                    self.host
                );
                return Ok(PkgManager::ApkNative);
            }
            anyhow::bail!(
                "apk add opkg failed (exit {}): {}",
                add_code,
                add_out.trim()
            );
        }

        anyhow::bail!(
            "opkg not found at /usr/bin/opkg — this router's firmware may not \
             support package management (TollGate requires a standard OpenWrt build)"
        );
    }

    /// `opkg update` — refresh package lists.
    pub fn opkg_update(&self) -> Result<()> {
        info!("[{}] opkg update", self.host);
        self.run_ok("/usr/bin/opkg update")?;
        Ok(())
    }

    /// Install a package, skipping if already installed.
    pub fn opkg_install(&self, package: &str) -> Result<()> {
        // Check if already installed to avoid unnecessary network traffic.
        let (_, code) = self.run(&format!(
            "/usr/bin/opkg list-installed | grep -q '^{} '",
            package
        ))?;
        if code == 0 {
            info!("[{}] {} already installed", self.host, package);
            return Ok(());
        }

        info!("[{}] opkg install {}", self.host, package);
        self.run_ok(&format!("/usr/bin/opkg install {}", package))?;
        Ok(())
    }

    /// Remove a package.
    pub fn opkg_remove(&self, package: &str) -> Result<()> {
        info!("[{}] opkg remove {}", self.host, package);
        self.run_ok(&format!("/usr/bin/opkg remove {}", package))?;
        Ok(())
    }

    /// Install a standard OpenWrt package via whichever manager is active.
    ///
    /// Unlike `tollgate-module-basic-go` itself (which falls back to manual
    /// `.ipk` extraction and therefore skips dependency resolution — see
    /// `tollgate::install`), packages like `nodogsplash` are in every
    /// upstream OpenWrt feed, so a plain `apk add` / `opkg install` works
    /// even in `ApkNative` mode.
    pub fn install_package(&self, pkg_mgr: PkgManager, package: &str) -> Result<()> {
        match pkg_mgr {
            PkgManager::Opkg => self.opkg_install(package),
            PkgManager::ApkNative => self.apk_install(package),
        }
    }

    /// `apk add <package>`, skipping if already installed.
    pub fn apk_install(&self, package: &str) -> Result<()> {
        let (_, code) = self.run(&format!("apk info -e {} >/dev/null 2>&1", package))?;
        if code == 0 {
            info!("[{}] {} already installed", self.host, package);
            return Ok(());
        }

        info!("[{}] apk add {}", self.host, package);
        self.run_ok(&format!("/usr/bin/apk add {}", package))?;
        Ok(())
    }
}
