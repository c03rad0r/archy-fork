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
pass. Source is not yet deployed. This is not claimed as the operator's gate cause.

## Remaining acceptance

Reproduce the exact hostname/app in iframe and tab; qualify trusted TLS, HTTP LAN,
authenticated/expired/logged-out sessions, companion, service restart, hostname
regeneration and update persistence. Verify unauthenticated app access is still
challenged. Preserve the public-management source guard and Shorty containment.
