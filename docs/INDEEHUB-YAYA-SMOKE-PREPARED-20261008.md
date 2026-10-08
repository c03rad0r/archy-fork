# Yaya IndeeHub post-deployment smoke handover

Status: **prepared and source reviewed; NOT executed or a runtime pass.**

The reviewed harness, exact-source review receipt and user UAT checklist are
archived privately at:

`/home/archipelago/.local/state/archipelago/release-qualification/indeehub-yaya-smoke-prepared-20261008/`

- `indeehub-yaya-deployed-smoke.cjs`: actual deployed dashboard/app assets, existing
  private node session, 390px and 1440px checks; no route replacement.
- `indeehub-yaya-smoke-source-review.json`: 17 inspected frontend files match the
  delivered frontend source receipt at `5d0ea645`; API controllers/DTOs match
  the API source receipt at `3b09b81d`. Candidate node consent labels checked.
- `indeehub-yaya-deployed-smoke-UAT.txt`: run prerequisites and short user checklist.
  Its original `/tmp` artifact references identify the reviewed source copies;
  use the archived copies above if temporary files disappear.
- `archive-receipt.json` and `SHA256SUMS`: qualification limits and artifact hashes.

Only `node --check` syntax validation has run. Root reviewed the authentication
request guards, signing bounds, selectors and response shapes. No browser,
authentication, playback or live-node mutation ran for this harness. The archive
contains no cookies or bearer tokens; its private cookie-path reference must not
be replaced by an exported credential in documentation.

## Execution prerequisites and scope

Wait for reviewed Yaya deployment readiness and an explicit browser resource slot.
The harness requires `INDEE_DEPLOYMENT_READY=1` and
`ALLOW_REAL_AUTH_SIGNING=1` for the already-authorized authentication-only check,
plus the existing private cookie file and browser CDP session. If the session has
expired, stop; do not reset credentials or create another identity automatically.
Verify `SHA256SUMS` before execution and record runtime results in a new receipt,
without changing this prepared-only archive into a claimed pass.

Browse and Backstage are read-only. Authentication permits bounded existing-identity
challenges and exact NIP-98 login events only. Any unknown RPC, forbidden endpoint,
non-authentication write or non-login signature makes the run fail. Blocked
WebSockets and external media/relay reads remain explicit limitations.

Playback requires an existing published film with explicit zero project and
content prices plus backend confirmation of free pricing and existing storage,
then uses its real UI Play button. Selection is limited to the first 30 catalog
projects; no eligible reachable title means playback NOT_TESTED. Never substitute
personal Cloud media, including `web54321-balanced-2.mp4`, or create a publication.
A verified non-filmmaker identity may leave Backstage NOT_TESTED; other failures
must not be disguised as an access limitation. Physical-phone UAT remains separate.

Screenshots from a future run may contain private Backstage display content and
must stay in its private output directory. Do not publish this archive or runtime
screenshots as public release assets.
