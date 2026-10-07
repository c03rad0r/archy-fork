# Installer-owned IndeeHub registration identity

Status: source compiled and eight installer/orchestrator tests passed in the
combined isolated backend run. That run has one unrelated FIPS binding test
failure (1,821 passed, one failed, five existing skips), so combined qualification
is not clean. The separate container manifest test passed. No deployment
or actual-node acceptance. This adds an installation prerequisite; it does not
enable registration or publication.

The IndeedHub API manifest explicitly opts in with
`container.media_registration_identity: true`. The field defaults to false and
is omitted from serialized ordinary manifests. Only opted-in apps read the
existing node identity or receive registration identity variables.

The production install/reconcile environment chokepoint loads the existing node
key, verifies its derived public key matches `identity/node_key.pub`, and asks the
private installation store for a stable pin. Missing or mismatching identity
files fail; no node key is generated, repaired or replaced. The helper runs on a
blocking worker and uses bounded cross-process locking plus durable no-replace
writes.

The public identity values injected are:

- `ARCHIPELAGO_REGISTRATION_NODE_PUBLIC_KEY`: exact existing Ed25519 public key.
- `ARCHIPELAGO_REGISTRATION_NODE_DID`: its matching did:key identity.
- `ARCHIPELAGO_REGISTRATION_AUDIENCE`: one persisted UUID for this installed app.

These are public identity bindings, not secret signing keys. They are derived
from authenticated installation state. Manifest plaintext duplicates are replaced
by the authoritative pins; derived/secret-backed attempts to override these names
are rejected. Existing native-signing user identities are unrelated to this node
receipt-signing identity. `NODE_IDENTITY_PUBKEYS` is not used for this purpose.
No `_ENABLED` setting is written. No unrelated app receives these values.

## Persistence and recovery

The node keeps private 0600 records beneath the 0700 directory
`<node-data>/app-registration-pins/`:

- `<app-id>.json`: version, app ID, node public key/DID and stable app audience.
- `<app-id>.initialized.json`: version, app ID and canonical pin-record SHA256.

Provisioning returns only after both files and their directory are fsynced.
Interrupted initial provisioning after pin persistence finishes its marker on
retry without changing the audience. A missing pin with a surviving marker is
an error requiring recovery of the original pin, not permission to generate a
replacement. Damaged, unreadable, symlinked or mismatched files remain untouched.
A node identity change also requires an explicit migration; it does not silently
repin an existing app.

`load_existing` is the future bridge/RPC lookup: it never creates files or app
identity. It requires matching complete persisted records. The bridge must bind
the actual installed frontend/backend relationship: IndeedHub's UI app is
`indeedhub`, while its API and this stored registration scope are `indeedhub-api`.
Arbitrary apps must not select another installed scope by sending its name.

The audience survives app upgrade, ordinary restart, repeated reconciliation,
failed install retry and reinstall that preserves the app database. This change
contains no pin deletion API or uninstall hook. It does not alter uninstall
choices, persistent app data, original Cloud files, wallet data or outstanding
media snapshots/rentals.

Explicit full data deletion is a separate operator decision. A future reset
workflow must coordinate deletion of app database/intents and pin state, and must
preserve or resolve outstanding node media/payment obligations before allowing a
new audience. Removing only one pin file is not a supported reset. Merely
uninstalling/reinstalling an app must not rotate its audience. If all installation
records are externally destroyed, this store cannot distinguish that from a
first installation; restore them along with the original app database rather than
re-enabling registration against a new audience.

## Prepared local checks

The patch adds five isolated store/env tests, three production-orchestrator
chokepoint tests and one manifest test. They cover stable retry/reload, distinct
app audiences, concurrent provisioning, key preservation, corruption/missing-pin
and foreign-key rejection, interrupted marker completion, symlink rejection,
authoritative/idempotent environment rendering, no implicit enablement, unrelated
apps without identity, and existing-key/public-key consistency. Fixtures use
private temporary directories and mock runtime calls only.

The combined run used `scripts/test-backend-isolated.sh`; all five store/env and
three orchestrator tests passed. Log:
`/tmp/archy-indeehub-executor-combined-tests.log`. All 377 captured source/build
input hashes remained unchanged through the run; provenance is recorded in
`/tmp/archy-indeehub-executor-combined-provenance.json`. The single failure is
`fips::dial::tests::purchase_post_serializes_once_and_binds_the_actual_bytes_to_the_peer`.
Do not treat this as a clean combined suite. The container crate manifest test
still needs its scoped isolated run in the next coordinated slot. There is no
node deployment, catalog publication or app registration from this change.

Qualification update: the scoped isolated container run passed
`manifest::tests::media_registration_identity_is_explicit_and_preserved` (one
passed, zero failed, 82 filtered), with all 11 container/workspace input hashes
unchanged. Log: `/tmp/archy-registration-container-manifest-test.log`. The later
combined backend batch passed all installer tests again; its overall 1,834 passed,
one failed legacy missing-file expectation and five existing skips is not a clean
combined suite. Source remains undeployed.

## Combined qualification after resumed integration

The full isolated backend rerun passed 1,848 tests with zero failures and five
existing skips. All 385 recorded source, build and fixture hashes remained
unchanged. Evidence: `/tmp/archy-qualified-candidate-backend-rerun-tests.log` and
`/tmp/archy-qualified-candidate-backend-rerun-provenance.json`. Earlier failed
runs remain recorded. This qualifies the current backend primitives and Browse
path repair locally; production build and live deployment checks are next.
Complete purchase callers, app registration/publication and timed playback
acceptance remain open. No real payment or public publication was performed.
