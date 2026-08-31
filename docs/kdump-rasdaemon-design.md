# kdump + rasdaemon — post-mortem and hardware-error capture (#144)

Status: IMPLEMENTED (phase 1) — decisions approved 2026-08-30: hang capture ON,
crashkernel=256M, ship the backfill with this release, phase-2 UI deferred.
Delivery: image-recipe (Dockerfile.rootfs, auto-install.sh cmdline) +
`core/archipelago/src/host_fixups.rs` (existing nodes, see
docs/system-level-ota-design.md) + `tests/lifecycle/os-audit.sh` section D.
Owner: node image (image-recipe) + lifecycle gate
Issue: #144 — "Configure kdump and rasdaemon for troubleshooting"

## The problem

When a fleet node hard-locks or a memory stick starts failing, today we get
nothing: a frozen kiosk is power-cycled and the evidence is gone; a DIMM
throwing correctable ECC errors for weeks is invisible until it starts
corrupting things. Two standard kernel mechanisms capture this evidence:

- **kdump** — reserves a small crash kernel at boot; on a kernel panic (or,
  configured so, a hang) the running kernel hands the machine over to the
  crash kernel, which writes a compressed dump of memory to disk and
  reboots. The node comes back by itself *and* leaves a post-mortem.
- **rasdaemon** — a userspace daemon that records hardware error events
  (correctable/uncorrectable ECC per DIMM, PCIe AER) from EDAC/sysfs into a
  sqlite database: persistent evidence of degrading hardware with no crash
  required.

## Facts the design rests on

- Installed-disk layout (auto-install.sh): BIOS boot 1MiB · EFI 512MiB ·
  **root ext4 30GiB, unencrypted** · data (rest, LUKS).
- The data partition is LUKS and unlocked late by the node itself — the
  crash kernel must never be asked to handle key material.
- The installed system's kernel command line is written by
  auto-install.sh:1810 (`GRUB_CMDLINE_LINUX_DEFAULT="quiet splash …"`).
- Packages land via `Dockerfile.rootfs` (trixie) with `systemctl enable`
  in the same RUN block (nginx/tor/avahi pattern).
- Kernel cmdline cannot be changed by OTA — it lives in GRUB. Existing
  nodes need a backfill step (bootstrap) plus a deliberate reboot.

## Design

### kdump

- **Packages:** `kdump-tools kexec-tools` added to Dockerfile.rootfs.
- **Command line:** append `crashkernel=256M` to
  `GRUB_CMDLINE_LINUX_DEFAULT` in auto-install.sh. 256M covers the capture
  kernel plus makedumpfile on the fleet's 16–64GB amd64 machines (~1–2% of
  RAM reserved, permanently). The arm image (RPi, config.txt boot) is out
  of scope for phase 1.
- **Dump target:** `local filesystem /var/crash` — on the unencrypted 30GiB
  root, deliberately *not* the encrypted data partition. No key handling
  in the crash initramfs, no dependency on the node's own unlock logic.
- **Core collector:** `makedumpfile -l --message-level 1 -d 31`
  (compressed, zero/free pages excluded) — a dump lands at roughly 5–15%
  of RAM, i.e. ~1–2 GiB on a 16 GiB machine.
- **Retention:** keep the **2 newest** dumps only. A small systemd timer
  (or kdump-tools' `KDUMP_POST_SCRIPT`) prunes older vmcores; a full root
  partition is already caught by disk_monitor's usage tracking. Two dumps
  ≈ 4 GiB worst case on 30 GiB root — safe.
- **When to dump — the deliberate trade-off (decision needed):**
  - Baseline: dump on real panics (`kernel.panic` path) — no behavioral
    change to a wedged node.
  - Recommended for this fleet: also enable hang capture
    (`kernel.hung_task_panic=1`, hardlockup via NMI watchdog). A kiosk
    that hard-locks is useless until power-cycled anyway; converting the
    hang into "dump + automatic reboot" turns every freeze into evidence
    *and* self-heals the node. Cost: a genuinely-busy-but-alive machine
    that trips the watchdog reboots — the threshold is kernel-default
    conservative (40s), so this should be rare.

### rasdaemon

- **Packages:** `rasdaemon`; `systemctl enable rasdaemon` in the
  Dockerfile.rootfs enable block (same pattern as nginx).
- **Storage:** its default sqlite DB at
  `/var/lib/rasdaemon/ras-mc_event.db` on the unencrypted root.
- **Human access today:** `ras-mc-ctl --summary` / `--errors` over SSH.
  No UI in phase 1.

### Surfacing (phase 2 — separate follow-up, not in this cut)

A small read-only `system.diagnostics` surface: last-crash timestamp and
vmcore sizes from `/var/crash`, plus ECC error totals per DIMM from the
rasdaemon DB — shown in Settings → System. Deliberately deferred: capture
first, UI once there is something to show and a node in the fleet has
actually produced a dump.

### Existing nodes (phase 1.5 backfill)

The OTA cannot change the bootloader. Bootstrap (which already delivers
fixes to existing nodes) appends `crashkernel=256M` (and the chosen
panic/hang params) to `/etc/default/grub` on machines that don't have it,
and enables `rasdaemon` via the node's package install path. **Takes
effect on the next reboot** — the operator reboots nodes when applying the
release; no special ceremony needed beyond that.

## Testing

- Image: the new packages appear in the ISO; QEMU boot smoke
  (build-iso-release.sh stage 5) still green.
- Lifecycle gate additions (bats, archi-dev-box first): `kdump-config show`
  reports a loaded crash kernel reservation; `systemctl is-active
  rasdaemon`; `/etc/default/grub` carries `crashkernel=`.
- Live drill (once, on archi-dev-box, not in the gate): trigger
  `sysrq c` → vmcore appears in `/var/crash`, node reboots itself,
  second boot is clean. Keep this manual — it reboots the box.

## Implementation touchpoints

1. `image-recipe/build/auto-installer/Dockerfile.rootfs` — packages +
   `systemctl enable rasdaemon`.
2. `image-recipe/build/auto-installer/installer-iso/archipelago/auto-install.sh:1810`
   — append `crashkernel=256M` (+ hang params if approved) to
   `GRUB_CMDLINE_LINUX_DEFAULT`.
3. `kdump-tools` config: `/etc/default/kdump-tools` (dump target
   `/var/crash`, core_collector line, `KDUMP_POST_SCRIPT` or timer for
   retention).
4. Bootstrap backfill for existing nodes.
5. `tests/lifecycle` — presence assertions (crash kernel reserved,
   rasdaemon active).

## Decisions needed before implementation

1. **Hang capture on or off?** Recommended ON (`hung_task_panic=1` +
   NMI watchdog): every hard lockup becomes a dump + self-reboot. OFF
   means dumps only on true panics; wedged nodes still need the button.
2. **crashkernel=256M vs 320M** — 256M is the common default for
   16–64GB machines; 320M if we expect large io-heavy kernels.
3. **Backfill now or new-installs-only?** Recommended: ship the backfill
   with the next release so the whole fleet gains capture on reboot.
4. Phase-2 UI surfacing scope — confirm "later" so phase 1 stays small.
