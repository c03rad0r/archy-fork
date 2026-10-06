# DATUM on Archipelago

Packages OCEAN DATUM v0.4.1beta, pinned to upstream commit
`5b061233a3d3323771b2be98e17f543e59346619`. The local build context must ship at
`/opt/archipelago/docker/datum`; no published registry image is assumed.

## First launch

Install a Bitcoin node and allow it to synchronize, then install DATUM. Open its
app tile and set your own Bitcoin payout address in DATUM's configuration page.
The initial address is deliberately empty: upstream keeps the UI available while
waiting for a valid address instead of mining to somebody else's address.
The admin username is `admin`. The generated password is stored on the node at
`/var/lib/archipelago/secrets/datum-admin-password`; retrieve it locally as the
node administrator. Do not put it in miner passwords or share it with miners.

Point miners at `stratum+tcp://<node-LAN-hostname>:23334`. Use a unique worker
name for every miner, following upstream's payout/worker naming rules:
https://github.com/OCEAN-xyz/datum_gateway/blob/v0.4.1beta/doc/usernames.md
The default is pooled mining only; loss of the pool connection stops mining
rather than silently switching to solo mining. DATUM's web UI reports template,
Bitcoin and pool readiness; an HTTP health check only proves the UI is alive.

## Stable connections

Gashboard connects inside `archy-net` to `http://datum:7152`, using Podman's DNS
alias. Never copy a container IP into either app's configuration. Bitcoin's DNS
name is resolved from `BITCOIN_HOST` on each start, and the shared RPC secret and
DATUM admin secret are refreshed without discarding the operator's settings.

External miners connect to the **node**, not its container. Use a DHCP reservation
on your router and a LAN DNS name if the miner supports DNS. Some miners do not
support mDNS (`.local`); use the reserved LAN IP for those. Container DNS fixes
container recreation, while the reservation prevents the node's DHCP address
from moving. Neither setting requires host networking.

Only Stratum is published directly. The admin UI is loopback-bound behind the
Archipelago app gate and retains DATUM's admin authentication. The backend uses
upstream's block notification polling fallback, so installing DATUM does not
rewrite or restart Bitcoin to add a `blocknotify` command.

## Data and validation

Settings live in `/var/lib/archipelago/datum/config.json` with mode 0600. Preserve
that directory and the platform secrets when uninstalling/reinstalling.

Before catalog publication, validate install, setup, Bitcoin IBD and recovery,
accepted shares from a real miner, stop/start, container recreation, preserved-data
reinstall, backend restart and a controlled node reboot. Verify Gashboard recovers
after DATUM receives a different container address. These live-node checks are
separate from the local manifest/build checks and require a dedicated test node.

## Local validation (2026-10-06)

The pinned image builds on Linux/amd64. Its UI returns HTTP 200 while waiting
for setup, `/clients` rejects unauthenticated requests, and its config is 0600.
The container runs with read-only root, cap-drop ALL and no-new-privileges.
Three config regression tests cover empty first-run payout, preserved payout
policy (including explicit false settings), secret/DNS refresh and invalid input.
Gashboard successfully polls this image using digest authentication and reconnects
when its container IP changes. Manifest preflight and generated catalog drift
checks pass. The catalog entries in this branch are review candidates; no signed
catalog or image has been published. Real mining shares and full lifecycle acceptance remain required before release.

## Operator-authorized node deployment (2026-10-06)

Installed as rootless Podman apps on archi-dev-box and yaya-server, with the
manifest and build context staged in the runtime payload. Both nodes report
DATUM healthy. Chromium verified the normalized My Apps icon, title, Launch
button, and embedded native UI with its Config navigation on both nodes.

On yaya, a normal package restart recreated DATUM at 10.89.0.11 instead of
10.89.0.9. Its configuration checksum was unchanged, and the still-running
Gashboard resolved `datum` to the new address and resumed successful authenticated
polling. Existing app container IDs remained unchanged on both hosts.

The payout address remains unset. HTTP health proves that setup is available,
not that Bitcoin/pool readiness or accepted mining shares have been established.
No node reboot, IBD experiment, preserved-data reinstall, signed catalog release,
or registry publication was performed in this deployment.

The deployed platform drops manifest UI/icon metadata from its initial Installing
placeholder, briefly showing a disk-only app under Services. Once scanned, both
apps appear in My Apps with their declared icons and launch interfaces. A separate
platform fix is being prepared; do not claim the installation-placeholder issue
is fixed merely because the completed installation is displayed correctly.
