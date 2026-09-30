# Same-node Gitea sources in Portainer

Status: root cause reproduced and network repair verified in disposable Portainer
instances; final migration integration and release acceptance remain in progress.
This change belongs to the next signed catalog, OTA and ISO. It does not modify
published 1.8.21 artifacts.

## Confirmed cause

On the affected X250, Gitea 1.27.3 and Portainer 2.45.0 run in rootless Podman
5.4.2, managed by user Quadlet services. Gitea publishes HTTP on loopback and the
Archipelago app gate serves its public port. Gitea's public ROOT_URL already
matches that gate URL.

Portainer had no explicit network selection and Podman selected pasta. Its
network namespace contained the host's LAN address. A Git request to that same
LAN address therefore reached Portainer's namespace rather than the host gate:
connection refused before authentication. The exact smart-HTTP request from the
host returned 200 with `application/x-git-upload-pack-advertisement`. From
Portainer's actual namespace the LAN request was refused, while its host mapping
returned a Git advertisement and the expected branch tip. Direct container-IP
requests timed out. Container health and host-only HTTP checks missed the defect.

A disposable Portainer using `slirp4netns` successfully created a Source through
Portainer's own API, using the original LAN clone URL. Returning that fixture to
pasta reproduced the refusal; recreating with slirp repaired it while preserving
its account and saved Source. Restart also passed. The requested branch tip and
Compose file were read from that actual Portainer network namespace. No user
stack was deployed. Deployment addresses and repository details are kept outside
this public record.

## Source changes

- Declare Portainer's rootless `slirp4netns` mode in its manifest. No shared static
  container IP, host networking, all-interface backend publication or auth bypass.
- Keep Gitea's loopback HTTP backend and gate port; machine Git uses Gitea's
  authentication. Remove obsolete port-3000 nginx metadata/template and the old
  best-effort installer commands which silently rewrote app.ini and falsely
  claimed success. Gitea owns first-run setup and operator configuration.
- Existing Quadlet reconciliation applies Network= drift. Record a durable
  pending restart before updating the unit and clear it only after a successful
  restart, so failed reloads/restarts and management interruptions retry.
- Detect explicit rootless network-mode drift in the older Podman runtime too.
  Unspecified networks do not trigger inferred changes to unrelated apps.
- Portainer opts into `backup_on_network_change`. Before recreation, gracefully
  stop the app and archive its writable persistent bind mounts, including nested
  Compose state, once each. Runtime sockets are excluded. Save the previous
  Quadlet definition, where present. Archives live under the node data directory's
  private `migration-backups/<id>/` directory; state is never deleted. Backup
  failures resume the original service and fail the migration visibly.
- Keep Podman API and Quadlet bind/network behavior covered by actual-manifest
  tests. Docker remains a development fallback: it now preserves bind/protocol
  declarations and rejects Podman-only networking instead of silently changing it.

## Operator use and diagnostics

Use Gitea's advertised HTTP(S) clone URL in Portainer Sources, with the Gitea
username and token in the credential fields. On first-run Gitea setup, the public
base URL must match the origin opened through Archipelago (including its port).
Keep a deliberately configured HTTPS/domain origin when one exists. Do not use a
container IP or put a token into the URL. A private repository requires repository
read permission. A successful Source check fetches Git refs; it does not deploy
a stack or establish that a Compose build uses a desired application revision.

`scripts/check-portainer-git-source.py` calls Portainer's own read-only Source
connection test. Supply a private mode-600 JSON credential file containing
`api_key` or `jwt`, and optionally `git: {username, password}`. Pass
`--portainer-url`, `--repository-url` and `--credentials-file`. It does not create
Sources or stacks and prints no credentials or raw server errors. It distinguishes
Portainer login/API failures from Git connection refusal, timeout, DNS/TLS
failure, HTML/login interception and repository authentication failure. TLS
verification stays enabled and API redirects are refused.

## Upgrade and rollback

The signed catalog embeds manifests and overrides installed disk copies.
Capability-gated manifest variants keep the previous Portainer manifest as the
base for older daemons; only daemons supporting `network-migration-backup-v1`
select the network repair. This prevents catalog refresh from triggering an
unbacked recreation before the OTA is installed. A disk
edit alone cannot deliver this fix. Publish the matching catalog with the tested
runtime, then verify the generated unit, actual network mode and Source API.
Expect a Portainer interruption while the snapshot and recreation run; duration
depends on its saved state size.
Gitea does not need recreation or an app.ini rewrite for this repair.

Keep the previous trusted catalog/runtime for rollback. Restore that catalog
before restoring the saved `previous.container`, reloading user systemd and
starting Portainer; otherwise reconciliation will correctly reapply the new
manifest. The archive is a stopped-state emergency backup, not an instruction to
roll back a live database automatically. Restore it only with Portainer stopped
and after preserving any newer state. Do not replace Gitea data/config, keys,
repositories or the production Portainer database with disposable test data.

## Validation and remaining gates

- Disposable X250 Portainer Source API: old mode refuses; repaired mode succeeds;
  saved account/Source survive recreation; restart succeeds.
- Invalid Git credentials produce a repository-authentication error, distinct
  from TCP refusal. Requested branch and Compose file read from Portainer context.
- Combined backend suite including the reviewed paid-download PRs and catalog
  rollout guard: 1,602 passed, zero failed, four existing ignored
  tests, including stopped-state archive round trips and failure preservation. Container runtime suite: 78 passed.
  Five diagnostic regression tests passed; catalog regeneration is idempotent
  and the generated catalog has zero manifest metadata drift.
- Fresh managed Gitea and Portainer fixtures: authenticated private Source
  creation, invalid-token rejection, workstation clone/push and exact branch
  lookup from Portainer namespace passed.
- Still required before release: live automatic migration with the new runtime,
  snapshot/rollback verification and reversed install-order acceptance,
  lifecycle/reboot convergence, and signed-catalog delivery to the existing app.
  Record LFS/registry/SSH/browser checks and actual hardware/runtime coverage.
