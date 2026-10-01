# Blocking incident: Framework LND startup and false zero balance

Before starting or resuming work in this repository, read
`docs/incident-framework-lnd-startup.md` and check its status.

The user explicitly required this incident to take priority over later work.
While its status is OPEN:

- Surface this blocker at session start, including when the user asks for unrelated work.
- Prioritize investigation and repair on the actual Framework node. If access is
  unavailable, ask for it and continue useful offline investigation; defer unrelated
  work unless the user explicitly overrides this priority.
- A manual LND restart, a source patch, passing local tests, or publishing an OTA
  does not resolve the incident. Do not mark it fixed until the Framework's startup,
  Receive flow, and balance behavior are verified on the node, including a controlled
  reboot with access and recovery arrangements in place.
- Preserve wallet identity, wallet/channel databases, credentials, and backups.
  Never run wallet wipe/recreation as an automatic investigation or recovery step.
- Record evidence, changes, validation, and remaining work in the incident document.

This priority comes from the user's explicit instruction on 2026-09-15. It remains
in effect across sessions until the documented acceptance criteria are met or the
user explicitly changes it.

## Unit tests on a live node

Run backend unit tests through `scripts/test-backend-isolated.sh`. Do not run
unrestricted `cargo test` on a node with installed apps: older mocked-runtime
tests still reached real service commands. The runner isolates wallet data,
service buses, container storage, networking, and process IDs. Compilation with
`cargo test --no-run` is safe. Keep separately authorized live checks explicit.

## Active release regression checklist

Before resuming release work, read
`docs/post-1.8.22-regressions-20261001.md` and retain its unfinished tasks.
The operator requested that every reported issue be tracked, fixed and tested
before another OTA/ISO. Keep source/unit-test results separate from actual-node
acceptance. In particular, paid-file recovery must not send another payment,
and app cleanup must preserve wallets, persistent data and uninstall decisions.
Do not mark the new paid-file incident resolved merely because the earlier
Framework LND startup incident was closed.
