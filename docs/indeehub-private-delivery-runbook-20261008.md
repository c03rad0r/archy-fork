# IndeeHub private Yaya delivery — 8 October 2026

Status: prepared, **not activated**. This is private free/authenticated app testing;
paid viewing, publication and payment UAT remain separate. No funds or public
announcements are authorized by this runbook. No Yaya command has been executed
by preparing it.

## Frozen inputs and prerequisite evidence

- Full same-boot post-target rollback: operation `5bd06edc`, qualified fixture
  executable `dd94dc6d…`, embedded helper `6fc3f978…`, 2,031 backend tests passed.
- Successful full cutover and independent final checks now pass for `8b53579c`:
  Committed/cleanup complete, Released maintenance, exact seven target images and
  saved units, new runtime IDs, fresh DB/four-volume restore proofs and no holds.
  Historical `f39bd824` timed out before target startup under host pressure, then
  natively recovered cleanly; that failure is retained separately.
- Matching optimized backend build is in progress against all536 unchanged
  backend inputs from the 2,031-test snapshot. Do not substitute the test-profile
  executable as the production artifact.
- Unsigned delivery catalog: worker runtime evidence directory,
  `unsigned-full-candidate-with-worker-29627fc.json`, SHA256
  `9967d06c8809d69cce6057b9f45a630e9ce21fd5cd33541e352e23336cd8eb07`.
  Its frontend hooks exactly match qualified authoritative source, SHA256
  `64015f79ca84c604cecd22e9e4a892644d485988f163c01e3d47277a64282747`.
- Existing frontend/API import evidence is in
  `~/.local/state/archipelago/session-recovery/indeehub-yaya-private-staging-5d0ea64/`.
  Worker import is prepared only; its qualified OCI SHA256 is
  `6db29188c0e68ed8e9e8d84ea2e9c5c70695518e0930775bf6b3200d14d99527`.
- Preserve historical catalog signatures and all old failure journals. Require
  reviewed local/ngit/Gitea main and applicable release refs to match before
  catalog activation. Parent owns source acceptance and signature coordination;
  check the existing signature request before requesting a new one.

## Read-only preflight before any Yaya mutation

1. Verify actual hostname `yaya-server`, rootless Podman owner/storage, current
   backend artifact/helper hashes and free durable disk capacity. The qualified
   executable above is a fixture build, not an optimized release artifact.
2. Capture fresh seven-container IDs/images/start times, original Quadlet bytes,
   exact volume identities, package state, lifecycle lock/journals/holds, operator
   stop/uninstall intent, node identity/session hashes, active catalogs/drop-ins
   and unrelated app/service identities. Compare to the retained baseline; any
   unexpected drift requires review. Never print private manifests or secrets.
3. Verify frontend/API local alias@digest IDs against their import receipt. Verify
   the worker archive, all OCI blob hashes and exact qualified digest. Do not pull
   an unreviewed replacement or repoint an existing alias to another image.
4. Resolve the API registration manifest using the existing node identity through
   the qualified registration-pin path. Registration and publication flags remain
   false. Preserve existing secrets, JWT identity and public installation pin.
5. Generate a new exact original-hash-bound seven-member plan from fresh units,
   using the frozen candidate for frontend, API **and worker**, with existing
   dependency image digests preserved. Review the full unit/manifest delta.
   Merely changing image tags or reusing the archived October 7 plan is invalid.

The offline draft planner is in
`~/.local/state/archipelago/release-qualification/indeehub-delivery-tools-20261008/`.
It explicitly reports `activation_ready=false`: its original-state inputs are
archived, public registration pins are unresolved, and signature/import/live
checks remain open. Draft SHA256
`aae8cdeca470b30481042c62f040435aa2292180cb54f0f4496d86a2204c4b8b`.
All three delivery image pins and frontend hooks match the frozen candidate.
This corrects an old planner assumption that would otherwise retain the legacy
worker. It is preparation, not an executable deployment authorization flag.

## Qualified activation sequence

After full cutover qualification and source/signature gates, import only the
qualified worker with the reviewed importer, recording that app runtimes,
identity/session, intents, management service and catalogs are unchanged.
Deploy the reviewed matching backend/helper artifact through the existing
preserving deployment procedure. Install only the exact reviewed plan and
verified signed private candidate; preserve previous catalog/drop-in bytes.
Confirm catalog selection alone has not replaced any existing app runtimes.

Run **one** `package.update` for `indeedhub`. The native supervised operation must
capture local writable-layer recovery images, acquire its maintenance barrier,
drain all seven writers, create fresh coherent backups and prove fresh database
and volume restore before target startup. Do not substitute an ad hoc volume tar,
manually pause live writers, or perform a separate API update. Monitor the same
operation to a proven terminal outcome; a caller timeout is not proof that
background recovery failed or completed. Never start another update to recover
an unfinished one.

## Acceptance before handing over the test link

Require Committed with cleanup complete, released maintenance, exact saved target
units/images, seven healthy runtimes, and unchanged identity/session, volumes,
unrelated apps and operator intent. Verify original107 migrations are retained
with exactly3 approved additions, data/media preserved, new API settings/pins
actually used, one current provider script, and publication flags still false.

On Yaya's real desktop/mobile UI, check original Nostr login identity, Browse,
Backstage saved media/projects and free authenticated playback. Give the user
Yaya's actual launch link only after these checks pass. Keep paid checkout,
producer payout, discovery/announcement and timed paid viewing acceptance open;
none is required to spend funds merely to expose private app testing.

If failure occurs, use only the supported operation-bound native recovery and
inspect its exact journal/holds. Preserve data written after cutover; never
reinstall, wipe/recreate wallets, run migration-down or restore old DB/media over
new writes automatically. Retain private recovery images, backups and receipts.


## Concrete operator-signing preparation

The current worker-inclusive unsigned catalog has not been signed. The prior
October7 signing request was unanswered and named the older frontend/API-only
candidate; it cannot establish approval/signature for the current payload.
The independently reviewed helper is:
`~/.local/state/archipelago/release-qualification/indeehub-delivery-tools-20261008/sign-reviewed-worker-catalog.py`.
Helper SHA256: `2711cfc060b71dcb7c7397a62a65971645ff861e8db3670f960c5b13baf5c13f`.
Canonical unsigned payload SHA256:
`d373968ee7043242fc954c3b1f114e236dc21f8749ea584a00360bb4db48fe30`.

Read-only pinned-input/verifier preflight passes. Noninteractive signing refuses.
The helper uses hidden operator-terminal input, makes terminal-echo fallback
fatal, passes the phrase only through the pinned ceremony executable's stdin,
and verifies the expected root and exact payload. It preserves the immutable
unsigned input and creates a separate signed output without replacing an existing
file. No secret was read, signing performed or new operator request sent during
preparation. Finish the build/source/review gates before requesting this final
operator step; never request a mnemonic in chat.
