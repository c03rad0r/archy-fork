# HTTPS app launches — 6 October 2026

Status: OPEN for the operator's exact iframe-only report. Node URL/app clarification
is pending. Separate confirmed defects below are repaired or under qualification.

## Live Yaya TLS failure

Read-only inspection found `archipelago.service` runs as `archipelago`, while
`/etc/archipelago/ssl/archipelago.key` was `0600 root:root`. The gate log explicitly
reported permission denied loading its TLS material. The dashboard's privileged
nginx could still use the same leaf. HTTPS to File Browser's gated port reset;
HTTP remained reachable. This does not establish that every reported gate page
has this cause.

Changed only the existing key group/mode to `0640 root:archipelago`, retaining the
certificate and key. No app/nginx restart or identity rotation. The management
service account can read the key. A browser context with the existing authenticated
session loaded File Browser over HTTPS in an iframe on both dev and Yaya, HTTP200
and no gate page (`/tmp/archy-https-frame-probe-after-permissions.log`). Certificate
errors were ignored only in the isolated context; normal certificate trust, the
operator's hostname, companion and expired-session behaviour are still unverified.
Metadata rollback, if necessary, is root:root0600; it would reintroduce the fault.

Initial browser probes used an intercepted parent and Chromium blocked the private
network request. These were test-harness failures, corrected by navigating to the
real parent before injecting the test iframe. Logs remain `/tmp/archy-https-frame-
probe-2.log` and `...-3.log`; they are not reported as passing acceptance.

## Durable source changes under qualification

- Startup runs an embedded, idempotent permission repair so existing installations
  and binary-only updates converge without generating or exposing keys.
- Hostname certificate regeneration repairs the staged key before either live
  file changes. Failure leaves current material intact.
- First-boot provisioning and operator-authorized host-key rotation preserve the
  daemon's group-read permission rather than forcing the leaf back to root-only.
- The repair rejects symlinks, non-regular files and unexpected owners, does not
  provision absent keys, grants no group write/execute or world access, and keeps
  read-only owner mode where present. It uses the daemon account's primary group.

Six isolated Python permission tests pass. The real extracted ISO first-boot
script harness passes9cases and rotation harness passes8cases, now asserting the
service group/mode. Full isolated Rust qualification passes (see `/tmp/archy-wallet-storage-tls-full-tests.log`); no updated
backend or ISO has been deployed or published.

## Embedded runtime URL correction

Commit `88d473f6` fixes exact loopback authority handling for localhost, 127.0.0.1
and IPv6 loopback, without rewriting external hostname/path substrings. Two
regressions reproduced the old failures;22focused tests and production UI build
pass. This UI correction is now deployed to dev and Yaya; it is not claimed
as the operator's exact gate cause.

## Remaining acceptance

Reproduce the exact hostname/app in iframe and tab; qualify trusted TLS, HTTP LAN,
authenticated/expired/logged-out sessions, companion, service restart, hostname
regeneration and update persistence. Verify unauthenticated app access is still
challenged. Preserve the public-management source guard and Shorty containment.

### HTTP/HTTPS session-boundary matrix

A follow-up owned-browser check on dev and Yaya passed all12 cases: HTTP and
HTTPS iframe loads each with authenticated, missing and invalid sessions.
Authenticated File Browser responses were200 without the gate page; missing and
invalid sessions remained401. Evidence: `/tmp/archy-https-frame-auth-matrix.log`.
The HTTPS diagnostic still explicitly bypasses certificate trust only in its
isolated browser contexts. This does not qualify the user's exact hostname/app,
physical companion, normal trust or expired-session renewal. No live config or
app state was changed by this matrix.

### Dashboard deployment and repeat checks

The UI from88d473f6 is deployed on dev and Yaya. Served index SHA256 is
`29589517ea9eb6ebd8722a3dd1113a5b597dd6845e393fbf384e17732d1d14df`.
Deployment verified unchanged backend bytes, session key and app container
identities/start times. Each node has a protected UI backup and rollback script.
Evidence: `/tmp/archy-https-runtime-ui-dev-deploy.log` and
`/tmp/archy-https-runtime-ui-yaya-deploy-final.log`.

Four delayed app-loading cases per node pass at390/1440 widths, in embedded and
overlay modes. After deployment, all12 HTTP/HTTPS valid/missing/invalid-session
iframe cases pass again. Logs: `/tmp/archy-https-ui-dev-browser.log`,
`/tmp/archy-https-ui-yaya-browser-recheck.log` and
`/tmp/archy-https-frame-auth-after-ui-recheck.log`.

Initial post-deployment browser attempts timed out and are retained as failures.
A diagnostic context without the dashboard's local authentication state landed
on Login; fresh authenticated contexts rendered both apps without page errors.
The matrix harness now catches its response timeout and seeds authenticated state
explicitly. Build-time disk/memory pressure was also measured; the owned build
was lowered in CPU/I/O priority without changing services. Neither observation
proves the cause of every timeout. Normal certificate trust, the exact reported
hostname/app, physical companion and restart/update persistence remain open.

### Certificate identity validation (without bypass)

Using each node's existing public trust material explicitly, dev's LAN IP
validates at443(HTTP200) and8083(HTTP401 without authentication). Yaya has no node
CA files: its legacy self-signed leaf contains only generic local DNS names and
127.0.0.1. Even when that leaf is explicitly trusted, its LAN IP fails certificate
identity validation (curl60); a DNS name actually listed on that certificate
validates and correctly returns401 at the app port. No certificate was replaced.

This is an additional confirmed limitation, not proof of the reported exact
iframe gate cause. `scripts/setup-node-ca.sh` provides the intended node CA flow,
but review found its live key/cert install and nginx-listener edits need safe
staging/rollback before using it as repair. Preserve existing CA identities and
custom certificates; do not blindly regenerate trust. A client must explicitly
trust the node's public CA for normal browser validation. Keep normal-trust
acceptance open pending a tested provisioning repair and the operator's access URL.

### Dev nginx reload mismatch found during the integrated purchase rollout

On 7 October UTC, the new backend wrote its playback proxy route but requests
still reached the old SPA. `nginx -t` and `systemctl reload nginx` both reported
success; the master error log showed wildcard IPv4/IPv6 port 443 bind failures.
Tailscale owned its tailnet port 443, while the old nginx workers still served the
original address-specific LAN/WireGuard listeners. The wildcard disk configuration
was already present in the pre-deployment backup.

`sites-available/archipelago` and `sites-enabled/archipelago` were separate regular
files. Both were backed up, and only their canonical wildcard HTTPS listeners
were changed to the two addresses already served by nginx: 192.168.63.240 and
10.44.0.1. Validation and reload then succeeded in practice: the new proxy returned
401 for unauthenticated GET and 405 for HEAD/POST, and owner RPC access passed.
Tailscale was not restarted or reconfigured. Original configs are in the dev
`support/integrated-purchase-backend-20261007T030942Z-2791483` backup directory.

Durable source/upgrade handling remains required before release: preserve the
recognized address-specific node-HTTPS profile when a template is installed,
account for enabled files that are not symlinks, and verify effective route/listener
behavior rather than treating a successful reload command as proof that nginx
accepted the new configuration. The existing per-address retarget helper ignores
wildcard-only configs. This live repair is not a claim that the general migration
or IPv6 HTTPS/companion trust acceptance is complete.
