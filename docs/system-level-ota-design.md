# System-Level OTA — host fixups

Status: Implemented (first payload shipped alongside this doc)
Owner: `core/archipelago/src/host_fixups.rs`
Related: docs/kdump-rasdaemon-design.md (first payload), CLAUDE.md invariants

## The problem

The binary OTA updates the node's own software, and the signed app catalog
updates apps. But the **host OS** — Debian packages, kernel parameters,
system services — previously moved only through ISO re-installs. A node
deployed a year ago can be running today's node software on a host that
never gained anything the image learned since. Issue #99 (missing polkit
rule on old nodes) and the audio-stack heal were each hand-carved
one-off bootstrap repairs; there was no general channel and no stated
policy for touching the host from the node.

## The mechanism

`host_fixups::ensure_host_fixups()` — spawned from `main.rs` at startup
alongside the other `ensure_*` heals, in the background, best-effort:

1. **Dev-box guard** — skip when `/home/archipelago/archy` is a symlink
   (contributor checkout) and when there's no dpkg (non-Debian host).
2. **Packages** — install only what's missing, from a curated, in-code
   list (`HOST_PACKAGES`), `apt-get install` first, one `apt-get update`
   retry, both under timeout, never fatal (offline/locked-dpkg nodes
   converge on a later boot).
3. **Configuration** — idempotent per-concern helpers writing root-owned
   config (via the existing `host_sudo` path): sysctl drop-ins, service
   defaults, GRUB cmdline, service enablement.
4. **Reporting** — every step logs what it did; failures log warnings and
   move on. A host fixup must never be able to stop the node from starting.

### Why embedded-in-the-binary rather than fetched

Same reasoning as the tor-helper (`bootstrap.rs`): the signed binary OTA
is the only authenticated delivery channel every node already trusts and
pulls on schedule. Fixups compiled into the binary travel with a version,
are reviewable in git, and can't be served to a subset of the fleet.

## Policy — what may travel this channel

| May | May not |
|---|---|
| Specific, pinned packages the node needs (kdump-tools, rasdaemon, …) | `dist-upgrade` or silent kernel/libc swaps — regular Debian upgrades stay with the operator |
| Kernel *parameters* via GRUB/sysctl — with the next-reboot caveat logged loudly | Anything requiring a secret, or touching LUKS key material |
| Service enablement + config the image also bakes in | Divergence: the ISO must converge to the SAME end state so fresh installs are a no-op |
| Small, reviewable, per-concern Rust functions with tests | Shell-script-of-things payloads beyond a single concern |

The rule: **the ISO and the fixup must express the same intent twice,
in reviewable places** — Dockerfile.rootfs/auto-install.sh for fresh
installs, `host_fixups.rs` for the deployed fleet. A change that lands in
one and not the other is a bug.

## Kernel cmdline caveat

`crashkernel=` (and any future `hugepages=`-style reservation) only takes
effect at boot: the fixup writes `/etc/default/grub` + `update-grub` and
logs `takes effect on the NEXT reboot`. Operators reboot nodes when
applying releases; no special ceremony is required beyond that, but the
lifecycle gate grades this state honestly (WARN for written-but-not-yet-
rebooted, FAIL for never-written — see `tests/lifecycle/os-audit.sh`
section D).

## Verification story

- Unit tests pin the policy constants and script shapes
  (`host_fixups` tests in `core/archipelago`).
- `tests/lifecycle/os-audit.sh` section D asserts the end state on a real
  node (config present, crashkernel reserved or pending reboot, hang
  policy live, rasdaemon active).
- The lifecycle gate runs on archi-dev-box per release; the QEMU ISO
  smoke covers fresh installs.

## Future payloads (candidates, not commitments)

- `unattended-upgrades` posture + a default-deny host nftables ruleset
  (the §F hardening-plan item — needs its own design first).
- Host firewall rules for mesh/WG ports.
- Chronic: anything the image learns post-deploy that old nodes must
  converge on (the polkit and audio precedents, formalized).
