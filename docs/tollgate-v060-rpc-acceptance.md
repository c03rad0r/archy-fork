## Live RPC acceptance — BOTH lanes

Run through `openwrt.provision-tollgate` against a real Archipelago node
(`archipelago 1.9.1-alpha-dev` built from this branch), driving a QEMU/KVM
OpenWrt guest. Not a shell replay. Each lane was a clean image with a working
route, resolver and package index.

### apk lane — OpenWrt 25.12.5 x86/64

```
opkg not found — using apk (OpenWrt 25.x+)
tollgate-wrt not installed — installing 0.6.0
detected arch: "x86_64" target: "x86-64"
package verified against SHA256SUMS (sha256:82d7dff1957ea6e17b1c5e352162c13eccab7678ba7e1be28eb43c2d04ffe8bb)
tollgate-wrt 0.6.0-r1 is installed and matches the 0.6.0 pin
```

Router read-back: `tollgate-wrt-0.6.0-r1 x86_64 {/feed/net/tollgate-wrt} [installed]`,
`/usr/bin/tollgate-wrt` = 13,508,600 bytes.

### opkg lane — OpenWrt 24.10.8 x86/64

```
Starting TollGate provisioning
opkg update
Installing tollgate-wrt
tollgate-wrt not installed — installing 0.6.0
package verified against SHA256SUMS (sha256:8789811201a96296ee0be5132a07400eeed84ce64a6115b06ced3f8650bccf6a)
tollgate-wrt 0.6.0-r1 is installed and matches the 0.6.0 pin
```

Router read-back: `opkg list-installed` -> `tollgate-wrt - 0.6.0-r1`,
`/usr/bin/tollgate-wrt` = 13,508,600 bytes.

### `openwrt.get-status` on both

```
apk  lane: {"installed":true,"version":"0.6.0-r1","version_known":true,
            "pinned_version":"0.6.0","up_to_date":true,"pkg_manager":"apk"}
opkg lane: {"installed":true,"version":"0.6.0-r1","version_known":true,
            "pinned_version":"0.6.0","up_to_date":true,"pkg_manager":"opkg"}
```

Both sha256 values match the release `SHA256SUMS` **and** the independent
`api.github.com` per-asset digests.

### Still open after this run

1. `wifi down 2>&1; wifi up 2>&1` exits 127 (`ash: wifi: not found`) on a
   radio-less guest and aborts provisioning **after** the install succeeded.
   Pre-existing on `fork/main` (`core/openwrt/src/tollgate/mod.rs:117`), not
   introduced by this PR — but it means every re-provision of a radio-less or
   wifi-disabled router fails at the last step. Needs its own card.
2. The 0.5.0 -> 0.6.0 upgrade and a rollback are still unproven.
3. `--force-depends` vs `install_runtime_deps` is argued, not measured.
4. `opkg_install_forced` in `opkg.rs` is now unreferenced.
5. No physical router has run any of this.

## Upgrade path — measured

Same lab, opkg lane. Installed `0.6.0_rc6-r1` from the rc6 release asset, then
provisioned through the same RPC:

```
tollgate-wrt 0.6.0_rc6-r1 is installed, wanted 0.6.0 — upgrading
package verified against SHA256SUMS (sha256:8789811201a96296ee0be5132a07400eeed84ce64a6115b06ced3f8650bccf6a)
tollgate-wrt 0.6.0-r1 is installed and matches the 0.6.0 pin
```

The binary was genuinely replaced: rc6's `/usr/bin/tollgate-wrt` was 9,487,478
bytes as packaged, the post-upgrade file is 13,508,600 bytes, and
`opkg list-installed` reports `0.6.0-r1`.

Note the upgrade did **not** need the `--force-*` flags — a different upstream
version installs normally. That matters for what follows.

### The force flags act on a stale package database, not on a republished counter

The `--force-reinstall` doc comment used to claim it covered "`0.6.0-r1`
present, `0.6.0-r2` requested". That case never reaches the installer:
`normalise_version` strips the trailing `-r<digits>` counter, so the pair
compares equal, `decide_install` returns `Skip`, and no install command runs at
all. `version.rs` already asserted this (`drops_the_openwrt_packaging_release_counter`);
a new test, `a_republished_packaging_counter_does_not_trigger_a_reinstall`,
pins the consequence so the flags cannot be "fixed" to cover it.

What the flag *does* do, measured directly: with the package database claiming
`0.6.0-r2` and `/usr/bin/tollgate-wrt` deleted by hand, a plain reinstall is
refused, while

```
opkg install --force-reinstall --force-overwrite --force-depends /tmp/rc6.ipk
```

reports `Installing tollgate-wrt (0.6.0_rc6-r1) to root... / Configuring
tollgate-wrt.` and restores the 13,508,600-byte binary. That is exactly the
damaged state the removed extraction path used to leave behind.
