//! Host-level fixups: OS packages, kernel parameters and system services the
//! node needs, delivered by the same signed-binary OTA that ships everything
//! else (docs/system-level-ota-design.md).
//!
//! Scope and posture — read before adding anything here:
//!
//! * **Idempotent + non-fatal.** Every step is a no-op when the host already
//!   has the desired state, and a failure (offline box, locked dpkg, missing
//!   package in the release's Debian suite) logs a warning and moves on. A
//!   host fixup must never be able to stop the node from starting.
//! * **Curated, pinned intent — not dist-upgrade automation.** We deliver the
//!   specific packages and settings a release deliberately adds (crash
//!   capture, hardware-error logging, later: unattended-upgrades posture, host
//!   firewall). Regular Debian upgrades stay with the operator; this channel
//!   never silently swaps a kernel or a libc.
//! * **Fresh installs converge too.** The ISO bakes the same end state in
//!   (Dockerfile.rootfs, auto-install.sh cmdline), so the fixup is a no-op on
//!   new machines and only does real work on already-deployed nodes.
//! * **Kernel cmdline can't move at runtime.** `crashkernel=` reserves memory
//!   at boot; the fixup writes GRUB and update-grub so the change lands on the
//!   next reboot, and says so in the log. Everything else (packages, sysctls,
//!   services) applies immediately.
//!
//! First payload (#144, docs/kdump-rasdaemon-design.md): kdump + rasdaemon —
//! post-mortem and hardware-error capture:
//!   * kdump-tools/kexec-tools/rasdaemon installed
//!   * /etc/default/kdump-tools: USE_KDUMP=1, dumps to /var/crash, compressed
//!     core collector
//!   * /etc/sysctl.d/99-archipelago-kdump.conf: a wedged node dumps and
//!     reboots rather than sitting dead until power-cycled
//!   * crashkernel=256M appended to the installed GRUB cmdline (next reboot)
//!   * /var/crash pruned to the two newest dumps
//!
//! The module is skipped on dev boxes (same guard bootstrap::run uses) and on
//! hosts without dpkg.

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::update::host_sudo;

/// Packages the node's host must have. Keep this list short and justified —
/// every entry is state we now own on the fleet's OS images.
const HOST_PACKAGES: &[&str] = &["kdump-tools", "kexec-tools", "makedumpfile", "rasdaemon"];

/// Crash-kernel reservation. 256M covers the capture kernel plus makedumpfile
/// on the fleet's 16–64GB amd64 machines (~1–2% of RAM, permanently reserved).
/// The arm image (RPi) is out of scope for phase 1 — see the design doc.
const CRASHKERNEL_PARAM: &str = "crashkernel=256M";

const KDUMP_SYSDROPIN_PATH: &str = "/etc/sysctl.d/99-archipelago-kdump.conf";
const KDUMP_SYSDROPIN: &str = "\
# Archipelago kdump policy (#144). A wedged kiosk is useless until someone
# power-cycles it — capture the evidence, then reboot by itself. Dumps land in
# /var/crash (see docs/kdump-rasdaemon-design.md); keep-2 pruning is done by
# the host fixup pass, not a timer.
kernel.panic = 10
kernel.panic_on_oops = 1
kernel.hung_task_panic = 1
kernel.hardlockup_panic = 1
";

/// How many dumps to keep in /var/crash. Two ≈ 4 GiB worst case on the 30 GiB
/// unencrypted root — the partition usage itself is tracked by disk_monitor.
const KEEP_DUMPS: usize = 2;

/// Entry point, spawned from main.rs at startup like the other ensure_* heals.
pub async fn ensure_host_fixups() {
    // Dev-box guard (same rationale as bootstrap::run): on contributor
    // machines /home/archipelago/archy is a symlink into a git checkout and
    // the host is the contributor's own OS — never touch it.
    let home_archy = std::path::Path::new("/home/archipelago/archy");
    if tokio::fs::symlink_metadata(home_archy)
        .await
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        debug!("/home/archipelago/archy is a symlink — skipping host fixups (dev box)");
        return;
    }
    // Non-Debian hosts: nothing we manage here applies.
    if tokio::fs::symlink_metadata("/usr/bin/dpkg").await.is_err() {
        debug!("no dpkg on this host — skipping host fixups");
        return;
    }

    if let Err(e) = run_host_fixups().await {
        warn!("host fixups failed (non-fatal): {:#}", e);
    }
}

async fn run_host_fixups() -> Result<()> {
    // 1. Packages — install only what's missing; a locked/offline apt must
    //    never block anything downstream (steps below degrade to no-ops).
    match ensure_packages().await {
        Ok(true) => info!("host fixups: installed missing packages"),
        Ok(false) => debug!("host fixups: all packages present"),
        Err(e) => warn!("host fixups: package install failed (non-fatal): {:#}", e),
    }

    // 2. kdump config + sysctl drop-in + GRUB cmdline + services. One helper
    //    per concern so a failure in one logs and leaves the others running.
    if let Err(e) = ensure_kdump_sysdropin().await {
        warn!(
            "host fixups: kdump sysctl drop-in failed (non-fatal): {:#}",
            e
        );
    }
    if let Err(e) = ensure_kdump_defaults().await {
        warn!(
            "host fixups: kdump-tools config failed (non-fatal): {:#}",
            e
        );
    }
    match ensure_crashkernel_cmdline().await? {
        true => {
            warn!("host fixups: crashkernel= written to GRUB — takes effect on the NEXT reboot")
        }
        false => debug!("host fixups: crashkernel already in GRUB cmdline"),
    }
    if let Err(e) = ensure_rasdaemon_enabled().await {
        warn!("host fixups: rasdaemon enable failed (non-fatal): {:#}", e);
    }
    if let Err(e) = prune_crash_dumps().await {
        debug!("host fixups: /var/crash prune skipped: {:#}", e);
    }
    Ok(())
}

/// True if any package was installed. Mirrors the polkit repair's apt posture:
/// install without `apt-get update` first; only if that fails (fresh suite,
/// stale index), update once and retry. Both under timeout, both non-fatal.
async fn ensure_packages() -> Result<bool> {
    // Package names are a fixed internal allowlist. Do not embed shell quote
    // characters in WANTED: quotes produced by variable expansion are data,
    // so dpkg-query would look for a package literally named 'kdump-tools'.
    let wanted = HOST_PACKAGES.join(" ");
    let script = format!(
        r#"
set -u
WANTED="{wanted}"
MISSING=""
for p in $WANTED; do
    dpkg-query -W -f='${{Status}}' "$p" 2>/dev/null | grep -q 'install ok installed' || MISSING="$MISSING $p"
done
[ -z "$MISSING" ] && exit 0
timeout 240 apt-get install -y --no-install-recommends $MISSING >/dev/null 2>&1 \
    || timeout 240 sh -c 'apt-get update >/dev/null 2>&1 && apt-get install -y --no-install-recommends $MISSING >/dev/null 2>&1' \
    || exit 3
exit 2
"#
    );
    let status = host_sudo(&["sh", "-lc", &script])
        .await
        .context("install host packages")?;
    match status.code() {
        Some(0) => Ok(false),
        Some(2) => Ok(true),
        code => anyhow::bail!("host package install exited with {code:?}"),
    }
}

/// Write the sysctl drop-in and apply it live (these four keys are all
/// runtime-settable, so the hang/panic policy takes effect without a reboot).
async fn ensure_kdump_sysdropin() -> Result<()> {
    let script = format!(
        r#"
set -u
PATH_FILE='{KDUMP_SYSDROPIN_PATH}'
CONTENT_FILE=/tmp/archy-kdump-sysctl.$$.tmp
cat > "$CONTENT_FILE" <<'SYSEOF'
{KDUMP_SYSDROPIN}SYSEOF
if [ -f "$PATH_FILE" ] && cmp -s "$CONTENT_FILE" "$PATH_FILE"; then
    rm -f "$CONTENT_FILE"
    exit 0
fi
mv "$CONTENT_FILE" "$PATH_FILE"
chmod 644 "$PATH_FILE"
sysctl --system >/dev/null 2>&1 || true
exit 2
"#
    );
    let status = host_sudo(&["sh", "-lc", &script])
        .await
        .context("write kdump sysctl drop-in")?;
    match status.code() {
        Some(0) => Ok(()),
        Some(2) => {
            info!("host fixups: installed {KDUMP_SYSDROPIN_PATH} (hang/panic policy)");
            Ok(())
        }
        code => anyhow::bail!("kdump sysctl drop-in exited with {code:?}"),
    }
}

/// Point kdump-tools at /var/crash with a compressed core collector. Works on
/// the package's shipped defaults file (USE_KDUMP=0, commented KDUMP_COREDIR)
/// and on any state we already wrote — pure line surgery, idempotent.
fn kdump_defaults_script(conf: &str) -> String {
    r#"
set -u
CONF='@@CONF@@'
[ -f "$CONF" ] || exit 3
CHANGED=0
# Remove the one malformed line emitted by the old systemd-run environment
# expansion bug before it was disabled. It makes every kdump-config invocation
# print an error while sourcing this file.
if grep -Fqx '=""' "$CONF"; then
    sed -i '/^=""$/d' "$CONF"
    CHANGED=1
fi
set_kv() {
    # Canonicalise KEY to one double-quoted assignment. Older fixup versions
    # could append duplicates because their exact-value check did not accept
    # double quotes; collapsing them also makes future passes idempotent.
    KEY="$1"; VAL="$2"
    EXPECTED="${KEY}=\"${VAL}\""
    COUNT=$(grep -c "^${KEY}=" "$CONF" 2>/dev/null || true)
    if [ "$COUNT" -eq 1 ] && grep -Fqx "$EXPECTED" "$CONF"; then
        return
    fi
    sed -i "/^${KEY}=/d" "$CONF"
    printf '\n%s\n' "$EXPECTED" >> "$CONF"
    CHANGED=1
}
set_kv USE_KDUMP 1
set_kv KDUMP_COREDIR /var/crash
set_kv CORE_COLLECTOR 'makedumpfile -l --message-level 1 -d 31'
[ "$CHANGED" -eq 1 ] || exit 0
systemctl enable kdump-tools >/dev/null 2>&1 || true
exit 2
"#
    .replace("@@CONF@@", conf)
}

async fn ensure_kdump_defaults() -> Result<()> {
    let script = kdump_defaults_script("/etc/default/kdump-tools");
    let status = host_sudo(&["sh", "-lc", &script])
        .await
        .context("configure kdump-tools")?;
    match status.code() {
        Some(0) => Ok(()),
        Some(2) => {
            info!("host fixups: kdump-tools configured (USE_KDUMP=1, /var/crash)");
            Ok(())
        }
        code => anyhow::bail!("kdump-tools config exited with {code:?}"),
    }
}

/// Set the installed GRUB cmdline to one fixed `crashkernel=` reservation and
/// run update-grub. Debian's kdump-tools package installs a grub.d snippet that
/// otherwise appends its own range-based reservation after ours; on amd64 that
/// silently wins and reserves only 192M instead of the intended 256M.
/// The reservation itself only exists after the next reboot — memory cannot
/// be set aside at runtime — so the caller must log the reboot caveat.
/// Returns true if the generated cmdline changed.
async fn ensure_crashkernel_cmdline() -> Result<bool> {
    let script = format!(
        r#"
set -u
GRUB=/etc/default/grub
KDUMP_GRUB=/etc/default/grub.d/kdump-tools.cfg
PARAM='{CRASHKERNEL_PARAM}'
[ -f "$GRUB" ] || exit 3
CHANGED=0
# kdump-tools sources this after /etc/default/grub and unconditionally appends
# crashkernel=512M-:192M. Neutralize that package default: Archipelago owns the
# explicit fixed reservation in GRUB_CMDLINE_LINUX_DEFAULT below.
if [ -f "$KDUMP_GRUB" ] && grep -qE '^[^#]*crashkernel=' "$KDUMP_GRUB"; then
    printf '%s\n' '# Archipelago owns crashkernel sizing in /etc/default/grub.' > "$KDUMP_GRUB"
    CHANGED=1
fi
LINE=$(grep -E '^GRUB_CMDLINE_LINUX_DEFAULT=' "$GRUB" | head -1)
[ -n "$LINE" ] || exit 3
# Remove any prior value before appending ours, so repeated fixups can never
# create conflicting parameters whose kernel precedence is easy to misread.
NEWLINE=$(printf '%s' "$LINE" | sed -E "s/[[:space:]]+crashkernel=[^ \"']+//g; s/\"$/ $PARAM\"/")
if [ "$NEWLINE" != "$LINE" ]; then
    sed -i "s|^GRUB_CMDLINE_LINUX_DEFAULT=.*|$NEWLINE|" "$GRUB"
    CHANGED=1
fi
[ "$CHANGED" -eq 1 ] || exit 0
timeout 120 update-grub >/dev/null 2>&1 || true
exit 2
"#
    );
    let status = host_sudo(&["sh", "-lc", &script])
        .await
        .context("set crashkernel= in GRUB")?;
    match status.code() {
        Some(0) => Ok(false),
        Some(2) => Ok(true),
        code => anyhow::bail!("crashkernel cmdline fixup exited with {code:?}"),
    }
}

async fn ensure_rasdaemon_enabled() -> Result<()> {
    let status = host_sudo(&["systemctl", "enable", "--now", "rasdaemon"])
        .await
        .context("enable rasdaemon")?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("systemctl enable --now rasdaemon exited with {status}")
    }
}

/// Keep only the newest [`KEEP_DUMPS`] dumps in /var/crash. Called on every
/// fixup pass rather than by a timer: the pass runs at every startup, which is
/// exactly the cadence at which new dumps appear (a dump ends in a reboot).
fn crash_dump_prune_script() -> String {
    format!(
        r#"
set -u
DIR=${{ARCHIPELAGO_CRASH_DIR:-/var/crash}}
[ -d "$DIR" ] || exit 0
KEEP={KEEP_DUMPS}
# kdump-tools keeps its lock and kexec command files beside timestamped dump
# directories. Count and prune directories only: treating those bookkeeping
# files as dumps can delete the sole freshly captured vmcore on startup.
COUNT=$(find "$DIR" -mindepth 1 -maxdepth 1 -type d -printf . | wc -c)
[ "$COUNT" -gt "$KEEP" ] || exit 0
find "$DIR" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\0' \
    | sort -zrn \
    | tail -z -n +"$((KEEP + 1))" \
    | cut -z -d ' ' -f 2- \
    | xargs -0r rm -rf --
exit 2
"#
    )
}

async fn prune_crash_dumps() -> Result<()> {
    let script = crash_dump_prune_script();
    let status = host_sudo(&["sh", "-lc", &script])
        .await
        .context("prune /var/crash")?;
    match status.code() {
        Some(0) => Ok(()),
        Some(2) => {
            info!("host fixups: pruned old dumps in /var/crash (keep {KEEP_DUMPS})");
            Ok(())
        }
        code => anyhow::bail!("/var/crash prune exited with {code:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysctl_dropin_carries_the_full_hang_capture_policy() {
        for key in [
            "kernel.panic = 10",
            "kernel.panic_on_oops = 1",
            "kernel.hung_task_panic = 1",
            "kernel.hardlockup_panic = 1",
        ] {
            assert!(KDUMP_SYSDROPIN.contains(key), "drop-in missing {key}");
        }
    }

    #[test]
    fn package_list_is_exactly_the_kdump_rasdaemon_set() {
        assert_eq!(
            HOST_PACKAGES,
            &["kdump-tools", "kexec-tools", "makedumpfile", "rasdaemon"]
        );
    }

    #[test]
    fn crashkernel_param_is_sized_and_unprefixed() {
        assert_eq!(CRASHKERNEL_PARAM, "crashkernel=256M");
    }

    #[test]
    fn keep_dumps_is_two() {
        assert_eq!(KEEP_DUMPS, 2);
    }

    #[test]
    fn kdump_defaults_repairs_old_malformed_line_and_is_idempotent() {
        use std::{fs, process::Command};

        let root = tempfile::tempdir().unwrap();
        let conf = root.path().join("kdump-tools");
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        fs::write(bin.join("systemctl"), "#!/bin/sh\nexit 0\n").unwrap();
        assert!(Command::new("chmod")
            .args(["+x"])
            .arg(bin.join("systemctl"))
            .status()
            .unwrap()
            .success());
        fs::write(
            &conf,
            "# package defaults\n=\"\"\nUSE_KDUMP=0\nUSE_KDUMP=\"1\"\n",
        )
        .unwrap();

        let script = kdump_defaults_script(conf.to_str().unwrap());
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let first = Command::new("sh")
            .args(["-lc", &script])
            .env("PATH", &path)
            .status()
            .unwrap();
        assert_eq!(first.code(), Some(2));
        let repaired = fs::read_to_string(&conf).unwrap();
        assert!(!repaired.lines().any(|line| line == "=\"\""));
        assert_eq!(repaired.matches("USE_KDUMP=").count(), 1);
        assert!(repaired.contains("USE_KDUMP=\"1\""));
        assert!(repaired.contains("KDUMP_COREDIR=\"/var/crash\""));
        assert!(repaired.contains("CORE_COLLECTOR=\"makedumpfile -l --message-level 1 -d 31\""));

        let second = Command::new("sh")
            .args(["-lc", &script])
            .env("PATH", path)
            .status()
            .unwrap();
        assert!(second.success());
        assert_eq!(fs::read_to_string(conf).unwrap(), repaired);
    }

    #[test]
    fn crash_pruning_ignores_kdump_bookkeeping_files() {
        use std::{fs, process::Command};

        let root = tempfile::tempdir().unwrap();
        let crash = root.path();
        fs::write(crash.join("kdump_lock"), []).unwrap();
        fs::write(crash.join("kexec_cmd"), "kexec -p").unwrap();

        for (name, epoch) in [("old dump", "100"), ("middle", "200"), ("newest", "300")] {
            let path = crash.join(name);
            fs::create_dir(&path).unwrap();
            fs::write(path.join("vmcore"), name).unwrap();
            assert!(Command::new("touch")
                .args(["-d", &format!("@{epoch}")])
                .arg(&path)
                .status()
                .unwrap()
                .success());
        }

        let status = Command::new("sh")
            .args(["-lc", &crash_dump_prune_script()])
            .env("ARCHIPELAGO_CRASH_DIR", crash)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(2));
        assert!(!crash.join("old dump").exists());
        assert!(crash.join("middle").join("vmcore").exists());
        assert!(crash.join("newest").join("vmcore").exists());
        assert!(crash.join("kdump_lock").exists());
        assert!(crash.join("kexec_cmd").exists());
    }

    #[test]
    fn crash_pruning_does_nothing_when_only_bookkeeping_files_exist() {
        use std::{fs, process::Command};

        let root = tempfile::tempdir().unwrap();
        for name in ["kdump_lock", "kexec_cmd", "another-marker"] {
            fs::write(root.path().join(name), []).unwrap();
        }
        let status = Command::new("sh")
            .args(["-lc", &crash_dump_prune_script()])
            .env("ARCHIPELAGO_CRASH_DIR", root.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
    }
}
