---
name: archipelago
description: Configure, troubleshoot, and safely modify an Archipelago node or its developer environment using the repository's typed operations, ownership rules, and recovery workflow.
metadata:
  short-description: Work safely on Archipelago systems
---

# Archipelago system work

Use this skill for node configuration, terminal/developer setup, service
diagnosis, network and SSH work, supported app operations, and system changes.
For app implementation load `archipelago-app`; for any new or changed UI also
load `archipelago-design`.

Read [system-map.md](references/system-map.md) before acting. Read `AGENTS.md`
and the relevant project documentation in the current checkout.

## Workflow

1. Identify the node, repository/worktree, owner, and whether the request is
   planning, source work, a disposable test, or a live-node operation.
2. Inspect current state before changing it. Prefer `archy`/Archipelago RPC
   operations and existing scripts over direct edits or ad-hoc service commands.
3. Preserve wallets, app data, credentials, user uninstall decisions, and
   unrelated services. Back up only the scoped state before a mutation.
4. Make the smallest reversible change. Keep generated state separate from
   user overrides; never edit generated or packaged files when an owned source
   or managed override exists.
5. Verify the actual result and report source tests, disposable integration,
   live-node acceptance, and packaged-artifact checks separately.

For repository work, use a dedicated worktree and focused branch. For backend
unit tests use `scripts/test-backend-isolated.sh`; do not run unrestricted
`cargo test` on a node with installed apps. Do not publish OTA, ISO, catalog,
or Git mirrors from this skill unless that publication is explicitly requested
and every release gate is satisfied.

## Terminal and sessions

Treat terminal close as detach. Resume the existing named session; never create
a duplicate shell or replay a lost command. A reboot can restore workspace and
Codex conversation metadata but cannot restore the old process. Distinguish
running, detached, ended, and interrupted states in user-facing output.

Keep credentials, wallet material, terminal output, and command contents out of
diagnostics and support bundles. A terminal is a privileged capability: verify
the authenticated owner, origin, attachment grant, and account boundary before
any PTY bytes flow.

## References

- [system map and safe recipes](references/system-map.md)
- [app and design routing](references/routing.md)
