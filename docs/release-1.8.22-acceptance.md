# Archipelago 1.8.22-alpha acceptance

Source: `6d5f3ffb850bfd3dcd396bac986ba770935d1daa`.

## Verified application and runtime changes

- Full isolated backend suite: 1,617 passed, zero failed, four explicit opt-in exclusions.
- Frontend suite: 1,133 passed. Final frontend and AIUI production builds succeeded.
- Container suite: 79 passed. Catalog compatibility/trust, release manifest, build contexts, pruning, Lightning readiness, NPM migration, safe doctor, companion recovery and ISO doctor-overlay regressions passed.
- Six companion dashboard images built using the current registry.
- Final OTA backend SHA-256: `e108b78bbbd21cb7d5d47c8d0b7b9b19b63fb0c44678773603202440ec7d6f5b`.
- Final OTA frontend SHA-256: `2da485a2da75ff2fbe4aba52d6f217150e303be43a031723480c9c4ff9d43f41`.

## Live acceptance

The exact OTA bytes were deployed to the development box and ThinkPad X250.
Native Bitcoin/LND and the X250 production site retained their container identity
and start time during these final management deployments. Both boxes completed
12-minute observations including scheduled diagnostics with running containers
and persistent mounts unchanged. The orphaned Cuprate dashboard stayed absent.

Actual X250 Chromium kiosk: hard refresh, NPM Launch to the correct admin URL,
visible login/admin page, readable inline Bitcoin version choices and pruning
checkbox passed. No Bitcoin installation was triggered by this test.

Final desktop/mobile checks passed for Bitcoin's IBD dashboard, one Mempool card,
headless Phoenixd, LND waiting/unknown-balance behavior and all five transparent
AIUI embedding layers. Standalone AIUI retains its wallpaper.

Portainer's actual production network namespace fetched Git refs and the Compose
file after final deployment. Original mounts were preserved. Earlier disposable
fresh/reverse-install and migration/rollback tests, and the production host's
operator-initiated reboot check, passed.

Live Tor-only Cashu paid-file acceptance verified a one-satoshi net purchase,
change, rejected-payment refund, exact file bytes, Files access and free repeat
delivery. No native Bitcoin/LND funds were moved. PRs 161/162 are merged and
closed; the open pull-request list is empty.

## Boundaries

- Angor's real dev API, fees, block tip, CORS and rootless/headless configuration
  passed. Full-chain indexing remains dependent on initial Bitcoin sync finishing.
- Optional live AI providers, physical RNode hardware, the opt-in Reticulum TCP
  subprocess test and creation of a production Minibits profile were not run.
- The previously recorded unsafe-doctor incident changed X250 container start
  times before the final fix. Persistent databases were present after recovery,
  but no pre-incident cryptographic wallet-identity baseline was available.
  Do not describe recovery evidence as an exact pre-incident balance comparison.
- No claim of perfect behavior on every device, network or future failure is made.

## Raw ISO acceptance

The raw ISO is 2,755,072,000 bytes. SHA-256:
`cf7be6378dcd52f6f62774523341fa75dd453fa73a9cadff5390846f483e0140`.

Mounted-artifact smoke checks passed, including BIOS/UEFI boot files, live-boot
hooks, build contexts, current doctor overlay, crash-capture configuration,
version and frontend payload. The ISO backend and all 653 OTA frontend/runtime
files match exactly. AIUI metadata names the tested source commit.

A disposable QEMU/KVM x86_64 VM with UEFI firmware, 3 GiB RAM, two vCPUs, a fresh
64 GiB NVMe virtual disk and no network completed the full installation. This
covered partitioning, LUKS2 data encryption, swap, system configuration, UEFI
bootloader and initramfs generation. Cold boot with the ISO detached reached the
visible Welcome to Archipelago setup screen. The installed backend and both
historical/current doctor paths matched source hashes. Backend/nginx were active;
health reported RPC/sessions ready, crash recovery complete and version 1.8.22.
No wallet was initialized in this disposable VM.

The first automatic VM reboot selected the still-attached installer ISO. That
was corrected in the test configuration by detaching the ISO and explicitly
booting NVMe. It was not accepted as an installed-system boot. The subsequent
cold boot above is the successful acceptance run.

The dev native Bitcoin/LND identity/start-time baseline also remained unchanged
after the ISO build and VM acceptance.

## Publication pending

Artifacts are staged in a draft release. Upload/readback verification and the
operator's offline signatures must complete before promoting the fleet manifest
and signed app catalog or publishing the Git/ngit releases.
