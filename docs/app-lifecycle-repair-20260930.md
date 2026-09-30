# App lifecycle repair — 2026-09-30

Status: source repairs, optimized build, new-node recovery and scoped live
lifecycle acceptance verified. Full release gate remains pending.
These are next-release changes. Published 1.8.21 artifacts remain unchanged.

## Report

The operator reports that restarting an app can make it disappear, and a hard
refresh offers installation again. Newly installed apps sometimes fail to
connect in both embedded views and browser tabs. The new X250 additionally reproduced GitWorkshop disappearing during install
and Nginx Proxy Manager spending approximately 14 minutes at 70%. A disposable
app on the dev box exposed a separate restart failure.

## Findings and repairs

- Quadlet removes containers during stop/restart. The scanner protected existing
  in-memory entries but did not reconstruct an absent app on a fresh daemon.
  It now synthesizes stopped entries from the durable installed set, respecting
  uninstall records, normalizing container prefixes, and preserving cached
  metadata. Absence does not establish an image version or available update.
- Concurrent read/modify/write operations could lose installed-app records;
  in-place writes could expose truncated JSON to readers. Serialize writers,
  publish by atomic rename, and sync the file and parent directory. Legacy
  package install/uninstall success paths update the durable record too.
- Scans and lifecycle/progress operations could replace a newer model from an
  older snapshot. Use locked mutations for lifecycle/progress, and merge scan
  results only into entries unchanged since the scan's merge snapshot.
- Container running state and TCP accept alone did not establish HTTP readiness.
  Add explicit `ui-ready` based on bounded HTTP probes of the loopback upstream;
  reject connection failures and server errors, accept normal redirects and
  authentication challenges, and do not follow redirects or send credentials.
  Self-signed HTTPS apps are probed locally without certificate validation.
- The app gate swept new listeners only every 60 seconds. Wake that sweep
  immediately for a ready upstream whose declared gate port is not yet claimed,
  and withhold readiness until external and Tor listener claims exist.
- Fixed launch URLs could bypass suppressed runtime URLs. Enforce readiness in
  app cards, details, centralized embedded/browser launchers, and session frames.
  Starting/restarting clears readiness immediately. A waiting frame does not
  load an iframe and resumes when the backend reports readiness.

### New X250 findings

- The published ISO copied only `bitcoin-ui`, `lnd-ui` and `electrs-ui` build
  directories. GitWorkshop failed because `/opt/archipelago/docker/archipelago-source`
  was missing. Copy the complete docker source tree for bundled and unbundled
  ISOs, matching OTA packaging. Validate every manifest build context and
  Dockerfile in OTA staging, ISO staging and the mounted ISO smoke test.
- After restoring the omitted contexts, GitWorkshop's retained npm audit rejected
  newly reported brace-expansion, fast-uri and ip-address vulnerabilities.
  Refresh the existing pinned dependency patch, keeping the audit enabled.
  Clean install/audit (zero advisories), type-check, 152 upstream tests and
  subpath production build pass. The image builds on the X250 and `/healthz`
  returns 200. No wallet or Bitcoin container restart was needed.
- Nginx was receiving data, not frozen: over 1 GB read during the pull. It
  completed at 12:40:46 UTC after starting at 12:26:27; its web endpoint returns
  200. The orchestrated path previously labelled the entire download/build/start
  operation "Creating container" at 70%. Give that aggregate operation its own
  truthful label and earlier phase; no byte-level download estimate is claimed.
- Restore install progress immediately from an already-loaded server snapshot,
  so a new store created after hard refresh does not wait for another mutation.
- Replace the install modal's native version popup with inline radio choices.
  On this actual X250's Chromium 152 kiosk renderer, selection changes work,
  options have white text on dark backgrounds, and remain above pruning controls.
  Screenshot and browser assertions captured; no install confirmation was clicked.

### Restart safety

The disposable fixture restart at 12:38:05 UTC stopped its container, then
`ss | kill` in runtime port cleanup sent SIGTERM to the management daemon at
12:38:35. The daemon owned the gate listener on the same port at other addresses.
Systemd restarted management; Bitcoin and LND container IDs/start times were
unchanged. Remove port-owner kills and broad `pkill` patterns from restart,
install recovery and Grafana preparation. Recovery now uses the existing
container-ID-aware ghost reaper: absent container ownership must be established
before a process is terminated. A real listening-socket regression checks that
conflict cleanup preserves the host listener. App-gate manifest lookup now honors
`ARCHIPELAGO_APPS_DIR`, matching the orchestrator's configured manifest root.

## Validation

- Full frontend suite: 139 files, 1,126 tests passed; final focused kiosk/store
  checks: nine passed. Production frontend build passed.
- Final isolated backend suite: 1,567 passed, zero failed, four existing ignored
  tests. Optimized backend build passed and was deployed to the development node.
- Tests cover empty runtime inventory, alias deduplication, uninstall exclusion,
  concurrent durable writes, concurrent state changes, stale scan publication,
  TCP-without-HTTP, HTTP statuses including 502/503, and gate listener claims.
- Live disposable Node fixture delayed HTTP startup by 25 seconds. Desktop and
  mobile retained the waiting screen through hard refresh without mounting an
  iframe, then opened the exact fixture page automatically when ready.
- Restart retained the app in both state APIs throughout and returned to ready;
  the management PID did not change. Stopping removed the Quadlet container;
  restarting management reconstructed its installed/stopped entry without a
  false update offer. Starting it again succeeded. Desktop and mobile continued
  to show the installed app after hard refresh.
- LAN access required node authentication and returned exact fixture bytes after
  authentication. The fixture was uninstalled through the package lifecycle API;
  its temporary manifest root and service override were removed.
- Bitcoin and LND container IDs and start times stayed unchanged through all
  scoped checks and management restarts. No wallet data was used by the fixture.
- X250 kiosk checks also opened the repaired GitWorkshop and Nginx Proxy Manager
  pages successfully, with no failed local resource loads.

## Limits

This prevents the identified lifecycle/readiness failures; it cannot guarantee
that an app or network never fails after a successful readiness check. Actual
application failures must remain visible rather than being labelled successful.
The full lifecycle/reboot release gate and funded acceptance of the reviewed
paid-download PRs remain pending. The X250 kiosk fix has live rendering evidence.
