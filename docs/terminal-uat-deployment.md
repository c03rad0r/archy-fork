# Terminal UAT deployment runbook

Status: prepared, 2026-10-09. This runbook targets the physical Framework node
(`framework-pt`) and does not authorize an OTA, catalog publication, or wallet
mutation.

## Candidate contents

Build and record the backend binary, dashboard bundle, and source commit from
the isolated terminal worktree. The backend must have `ARCHY_SESSION_STATE_DIR`
set to the developer account's shared state directory (or use the default
resolution in `api/handler/terminal.rs`). Preserve the existing web root and
service binary before any replacement.

## Preconditions

1. Verify the Framework hostname and SSH host key through the operator's
   approved connection mechanism. A plain `ssh framework-pt` must not be used
   until host-key verification is available.
2. Capture service/container state, boot ID, running binary digest, served UI
   digest, and the existing `/var/lib/archipelago/support` layout without
   printing credentials, wallet files, or environment contents.
3. Create a timestamped protected rollback directory under
   `/var/lib/archipelago/support/terminal-uat-<timestamp>`.

## Acceptance flow

- Open the dashboard as the node owner; unauthenticated requests to
  `/api/terminal/sessions` and `/ws/terminal` return 401.
- Create a named session, type `printf 'uat\n'`, close the terminal, reopen it,
  and resume the same session without a duplicate tmux process.
- Refresh the browser and reconnect after a temporary network interruption.
- Open a second owner browser and verify inventory visibility; verify only one
  active attachment sends input at a time before enabling transfer controls.
- Confirm explicit End stops the tmux process but preserves the workspace.
- Reboot acceptance is separate: processes may stop, metadata must remain, and
  the UI must call this interrupted rather than a live resume.
- Verify the Omarchy-derived agent skill files and app starter are present in
  the candidate source/artifact; do not treat a local npm install failure as a
  successful app build.

## Rollback

Stop exposing the new dashboard before restoring the previous UI/backend pair.
Restore only from the protected receipt, verify the previous hashes and health,
and leave terminal session metadata/workspaces untouched unless the operator
explicitly requests session cleanup.
