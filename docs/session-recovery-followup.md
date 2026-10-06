# Stale CSRF recovery and honest network-interface status

Status: source/UI tests passed; backend qualification and deployment pending.

## Confirmed incident

During qualification on 2026-10-06, Yaya's existing dashboard and companion
sessions returned “CSRF token missing or invalid”. The Server page then reported
“No physical interfaces detected”. Read-only host checks showed working WiFi,
a default route, and external HTTPS 200. A freshly authenticated interface RPC
returned both Ethernet and WiFi correctly.

The earlier operator-session secret rotation performed by this agent had left
`remember_secret` owned by root with mode 0600, while the service runs as the
unprivileged node user. The current loader silently falls back to an in-memory
key when it cannot read/persist the file, so restart changed the CSRF derivation.
This was an error in the earlier repair operation, not a network outage.

Ownership is corrected on Yaya, the existing secret is preserved, and a newly
authenticated session remains valid after another management restart. Its kiosk
session was renewed through the normal password login cookies; the actual Server
page again displays Ethernet/WiFi and no longer shows the false empty message.
Dev's existing secret permissions were narrowed from 0644 to 0600 without
rotating its value. No network configuration or wallet service was changed.

## Candidate recovery

- Authentication and role checks still run before CSRF validation.
- A stale/missing CSRF value still returns 403 before dispatch. For a validated
  session only, that response sets the correct CSRF cookie and no-store headers.
- The client retries once when the rejection actually changed its CSRF cookie,
  including calls configured for a single attempt. Permissions failures and
  ambiguous network errors do not get this extra retry.
- Secret/session/expected-token prefixes are removed from CSRF diagnostics.
- Interface-fetch failures preserve known hardware and display an explicit
  failure/Retry action. Only a successful empty result says no hardware exists.

Focused RPC-client and Server view tests: 92 passed. The backend test exercises
an actual RPC handler: invalid CSRF must not create the settings file; corrected
retry succeeds; unauthenticated requests receive neither access nor a CSRF cookie.
That backend test still requires a completed isolated run before acceptance.

## Remaining gates

Verify actual HTTP cookie recovery with stale/missing CSRF, concurrent requests,
HTTP/HTTPS, a backend restart, and mobile/desktop network views. Verify the actual
companion once available, including the IndeeHub signer/launch path. Browser
fixtures alone do not constitute physical companion acceptance.

Retain follow-ups: durable remember-secret creation must be private and atomic;
unreadable/unpersistable keys must not silently become ephemeral successful
configuration; logout must expire/revoke remember credentials as intended.
Those broader hardening changes are not claimed by this recovery patch.
