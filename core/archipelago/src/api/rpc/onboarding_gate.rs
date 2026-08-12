//! Onboarding-posture gate for the unauthenticated, identity-mutating RPCs.
//!
//! Closes F-01 (Critical) of `docs/security/ENTROPY-SEED-AUDIT-2026-07-31.md`:
//! `seed.generate` / `seed.restore` / `seed.save-encrypted` /
//! `backup.restore-identity` / `auth.setup` all sit in
//! `middleware::UNAUTHENTICATED_METHODS`, and several of them reach
//! `NodeIdentity::from_seed` (`identity.rs:79-114`) or
//! `backup::identity::restore_encrypted_backup` (`backup/identity.rs:112-117`),
//! both of which overwrite `identity/node_key` unconditionally. Before this
//! gate, a single unauthenticated JSON-RPC POST from anywhere on the LAN — or
//! from any FIPS mesh peer — replaced a live node's Ed25519 identity, Nostr
//! node key and FIPS transport key.
//!
//! Those endpoints cannot simply be removed from the unauthenticated list:
//! they are *legitimately* pre-auth, because no user account exists until
//! `auth.setup` runs at the very end of the onboarding wizard. So instead of
//! authenticating the caller, this gate asks a different question — "is this
//! node still un-provisioned?" — and refuses once the answer is no.

use std::path::Path;

/// The D-04 sweep set: every method in `UNAUTHENTICATED_METHODS` that can
/// mutate node identity or credentials. Each of these either calls
/// [`ensure_onboarding_open`] or carries a written, evidence-backed verdict for
/// why it does not (`auth.onboardingComplete` takes the *opposite* guard — see
/// `api/rpc/auth.rs::handle_auth_onboarding_complete`).
///
/// This constant is the anti-drift anchor for `gate_calls_are_present`, the
/// source-guard test at the bottom of this file. It does not itself dispatch
/// anything, so it is dead in a non-test build by design — it exists to make
/// the sweep set reviewable in one place and to fail a test when a sixth door
/// is added without a gate.
#[allow(dead_code)]
pub(in crate::api::rpc) const IDENTITY_MUTATING_ONBOARDING_METHODS: &[&str] = &[
    "seed.generate",
    "seed.restore",
    "seed.save-encrypted",
    "backup.restore-identity",
    "auth.setup",
    "auth.onboardingComplete",
];

/// The refusal text. MUST begin with `Not supported:` — `sanitize_error_message`
/// (`middleware.rs:47-71`) only lets an error through to the caller when it
/// starts with a known prefix, and `Not supported` is already on that list.
/// Anything else would reach the operator as "Operation failed. Check server
/// logs for details.", which is a dead end rather than a refusal.
///
/// Kept under the sanitizer's 200-character truncation limit so the recovery
/// path (D-02) survives intact.
const REFUSAL: &str = "Not supported: this node is already provisioned. Re-keying requires the \
                       authenticated system.factory-reset, after which the normal onboarding \
                       restore flow works.";

/// Return `Ok(())` only while the node is still un-provisioned; otherwise
/// refuse with [`REFUSAL`].
///
/// # Signals (D-03 / D-03a)
///
/// Three independent signals, OR-ed. ANY one of them saying "provisioned"
/// refuses — the gate never trusts a single signal alone to say "open", which
/// is what makes it fail safe when the signals drift apart (a real state:
/// `auth.rs:193-217` carries auto-heal logic for exactly that drift).
///
/// | Signal | Source | Fresh node | Mid-onboarding | Provisioned |
/// |---|---|---|---|---|
/// | `AuthManager::is_setup()` (`auth.rs:116-119`, `user.json` exists) | disk | false | false | true |
/// | `AuthManager::is_onboarding_complete()` (`auth.rs:182-219`) | disk + flag | false | false | true |
/// | `crate::seed::seed_exists()` (`seed.rs:384-386`, `identity/master_seed.enc`) | disk | false | false | true (legacy nodes: false — covered by the other two) |
///
/// # Why `NodeIdentity::key_exists` is NOT one of them
///
/// The audit's suggested remediation, and the phase's own D-03, both named
/// `NodeIdentity::key_exists` (`identity.rs:117`) as the on-disk "this node is
/// onboarded" signal. **It is unusable, and a gate keyed on it would brick
/// first boot on every new node.** `Server::new` (`server.rs:63-71`) calls
/// `NodeIdentity::load_or_create` on *both* branches of its fresh-vs-existing
/// check, and `load_or_create` (`identity.rs:47-67`) generates and writes a
/// random temporary node key when none exists — its own comment says "Fresh
/// install — create a temporary identity. Onboarding will overwrite this with
/// seed-derived keys." So `key_exists` is `true` on every node that has booted
/// even once, onboarded or not, and refusing on it would refuse
/// `seed.generate` on a node that has never been onboarded.
///
/// `identity::fips_key_exists` was rejected for a related reason: the FIPS key
/// is written by `NodeIdentity::from_seed` (`identity.rs:108`), i.e. by the
/// *first* seed step, so it is already true midway through the wizard. Gating
/// on it would break a generate-then-restore switchback inside onboarding.
///
/// This correction is pinned by the test
/// `allows_on_fresh_temp_dir_even_though_node_key_exists`, not by this comment.
///
/// # Failure handling
///
/// An I/O error from any signal is treated as **provisioned** (fail safe), not
/// as open. A gate that opens when it cannot read the disk is not a gate.
///
/// # Disclosure
///
/// The refusal deliberately does not say *which* signal fired. A one-bit
/// "provisioned" answer discloses nothing beyond what `auth.isOnboardingComplete`
/// already discloses — that method is itself in `UNAUTHENTICATED_METHODS`
/// (`middleware.rs:9`) — but a per-signal breakdown would disclose more.
pub(in crate::api::rpc) async fn ensure_onboarding_open(
    data_dir: &Path,
    auth: &crate::auth::AuthManager,
) -> anyhow::Result<()> {
    // `unwrap_or(true)` is the fail-safe: an unreadable user.json or
    // onboarding.json means we cannot prove the node is fresh, so we refuse.
    let user_account_exists = auth.is_setup().await.unwrap_or(true);
    let onboarding_marked_complete = auth.is_onboarding_complete().await.unwrap_or(true);
    let encrypted_seed_on_disk = crate::seed::seed_exists(data_dir);

    if user_account_exists || onboarding_marked_complete || encrypted_seed_on_disk {
        // Log the deciding signals for the operator; the caller gets one bit.
        tracing::warn!(
            user_account_exists,
            onboarding_marked_complete,
            encrypted_seed_on_disk,
            "[onboarding-gate] refused an identity-mutating onboarding RPC on a provisioned node"
        );
        anyhow::bail!(REFUSAL);
    }
    Ok(())
}

/// The OPPOSITE guard, for `auth.onboardingComplete` only.
///
/// That method is unauthenticated and SETS the flag [`ensure_onboarding_open`]
/// reads, so without this an attacker could call it once against a fresh node
/// and permanently lock it out of onboarding — a denial of service created by
/// the gate itself (T-10-04). Onboarding cannot legitimately be "complete"
/// before a user account exists, so refuse until it does.
///
/// Failure handling is the mirror image of the main gate: an unreadable
/// `user.json` means we cannot prove an account exists, so we refuse
/// (`unwrap_or(false)`). Refusing here is safe — the flag is also inferred by
/// `AuthManager::is_onboarding_complete`'s auto-heal path (`auth.rs:203-217`)
/// once the account is set up, so nothing depends on this RPC succeeding.
pub(in crate::api::rpc) async fn ensure_user_account_exists(
    auth: &crate::auth::AuthManager,
) -> anyhow::Result<()> {
    if !auth.is_setup().await.unwrap_or(false) {
        tracing::warn!("[onboarding-gate] refused auth.onboardingComplete — no user account yet");
        anyhow::bail!(
            "Not supported: onboarding cannot be completed before a user account exists. \
             Set a password first."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthManager;

    /// A temp data dir plus its AuthManager, in the state a genuinely fresh
    /// node is in: nothing written yet.
    fn fresh() -> (tempfile::TempDir, AuthManager) {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthManager::new(dir.path().to_path_buf());
        (dir, auth)
    }

    #[tokio::test]
    async fn allows_on_fresh_node() {
        let (dir, auth) = fresh();
        assert!(ensure_onboarding_open(dir.path(), &auth).await.is_ok());
    }

    /// Pins the D-03a scoping correction as a test rather than a comment:
    /// every booted node has a `node_key` on disk (`server.rs:63-71` ->
    /// `identity.rs:47-67`), so a gate keyed on `NodeIdentity::key_exists`
    /// would refuse onboarding on a node that has never been onboarded.
    #[tokio::test]
    async fn allows_on_fresh_temp_dir_even_though_node_key_exists() {
        let (dir, auth) = fresh();
        let identity_dir = dir.path().join("identity");
        std::fs::create_dir_all(&identity_dir).unwrap();
        std::fs::write(identity_dir.join("node_key"), [7u8; 32]).unwrap();

        assert!(
            crate::identity::NodeIdentity::key_exists(&identity_dir),
            "precondition: the boot-time node key is on disk"
        );
        assert!(
            ensure_onboarding_open(dir.path(), &auth).await.is_ok(),
            "a boot-time node_key must NOT be read as 'onboarded' — that would \
             brick first boot on every fresh node"
        );
    }

    #[tokio::test]
    async fn refuses_when_user_json_exists() {
        let (dir, auth) = fresh();
        auth.setup_user("password123").await.unwrap();

        let err = ensure_onboarding_open(dir.path(), &auth).await.unwrap_err();
        assert!(
            err.to_string().starts_with("Not supported:"),
            "refusal must survive sanitize_error_message: {err}"
        );
    }

    #[tokio::test]
    async fn refuses_when_onboarding_flag_set() {
        let (dir, auth) = fresh();
        // Writes onboarding.json even with no user account — the drift case.
        auth.complete_onboarding().await.unwrap();
        assert!(!auth.is_setup().await.unwrap(), "no user.json in this case");

        assert!(ensure_onboarding_open(dir.path(), &auth).await.is_err());
    }

    #[tokio::test]
    async fn refuses_when_encrypted_seed_on_disk() {
        let (dir, auth) = fresh();
        let identity_dir = dir.path().join("identity");
        std::fs::create_dir_all(&identity_dir).unwrap();
        std::fs::write(identity_dir.join("master_seed.enc"), b"ciphertext").unwrap();
        assert!(crate::seed::seed_exists(dir.path()));

        assert!(ensure_onboarding_open(dir.path(), &auth).await.is_err());
    }

    /// The refusal must reach the caller intact rather than being collapsed
    /// into "Operation failed. Check server logs for details.", and it must
    /// name the D-02 recovery path.
    #[tokio::test]
    async fn refusal_survives_the_error_sanitizer_and_names_the_recovery_path() {
        let sanitized = crate::api::rpc::middleware::sanitize_error_message(REFUSAL);
        assert_ne!(
            sanitized,
            "Operation failed. Check server logs for details."
        );
        assert!(
            sanitized.contains("system.factory-reset"),
            "the refusal must not be a dead end: {sanitized}"
        );
    }

    /// T-10-04: `auth.onboardingComplete` must not be usable to lock a fresh
    /// node out of its own onboarding.
    #[tokio::test]
    async fn onboarding_complete_guard_requires_a_user_account() {
        let (_dir, auth) = fresh();

        let err = ensure_user_account_exists(&auth).await.unwrap_err();
        assert!(
            err.to_string().starts_with("Not supported:"),
            "refusal must survive sanitize_error_message: {err}"
        );

        auth.setup_user("password123").await.unwrap();
        assert!(
            ensure_user_account_exists(&auth).await.is_ok(),
            "once the account exists, completing onboarding is legitimate"
        );
    }

    /// Anti-drift source guard: every method in
    /// [`IDENTITY_MUTATING_ONBOARDING_METHODS`] must still carry its guard in
    /// the handler body that serves it. Deleting any single
    /// `ensure_onboarding_open` call fails this test instead of shipping.
    ///
    /// Matching is done on source text rather than behaviour because the
    /// handlers are `RpcHandler` methods, and constructing an `RpcHandler`
    /// needs an orchestrator, port allocator, session store and metrics store.
    #[test]
    fn every_identity_mutating_method_still_carries_its_guard() {
        const SEED_RPC: &str = include_str!("seed_rpc.rs");
        const BACKUP_RPC: &str = include_str!("backup_rpc.rs");
        const AUTH_RPC: &str = include_str!("auth.rs");

        // method -> (source file, the fn whose body serves it, guard call)
        let coverage: &[(&str, &str, &str, &str)] = &[
            (
                "seed.generate",
                SEED_RPC,
                "async fn handle_seed_generate",
                "ensure_onboarding_open",
            ),
            (
                "seed.restore",
                SEED_RPC,
                "async fn restore_node_identity_from_words",
                "ensure_onboarding_open",
            ),
            (
                "seed.save-encrypted",
                SEED_RPC,
                "async fn handle_seed_save_encrypted",
                "ensure_onboarding_open",
            ),
            (
                "backup.restore-identity",
                BACKUP_RPC,
                "async fn handle_backup_restore_identity",
                "ensure_onboarding_open",
            ),
            (
                "auth.setup",
                AUTH_RPC,
                "async fn handle_auth_setup",
                "ensure_onboarding_open",
            ),
            // The opposite guard — see `ensure_user_account_exists`.
            (
                "auth.onboardingComplete",
                AUTH_RPC,
                "async fn handle_auth_onboarding_complete",
                "ensure_user_account_exists",
            ),
        ];

        for method in IDENTITY_MUTATING_ONBOARDING_METHODS {
            assert!(
                coverage.iter().any(|(m, ..)| m == method),
                "{method} is in the sweep set but no source guard covers it"
            );
        }

        for (method, source, signature, guard) in coverage {
            let start = source
                .find(signature)
                .unwrap_or_else(|| panic!("{signature} not found — did {method} get renamed?"));
            let body = fn_body(&source[start..]);

            assert!(
                body.contains(guard),
                "{method}: {signature} no longer calls {guard} — the F-01 gate was removed"
            );
        }
    }

    /// The `{ .. }` block of the function `src` starts with, by brace matching.
    /// Deliberately exact: a looser "up to the next fn" slice would let a
    /// neighbouring handler's guard call satisfy the assertion for a handler
    /// whose own guard had been deleted.
    fn fn_body(src: &str) -> &str {
        let open = src.find('{').expect("function has a body");
        let mut depth = 0usize;
        for (i, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &src[open..open + i + 1];
                    }
                }
                _ => {}
            }
        }
        panic!("unbalanced braces while scanning a function body");
    }

    /// The headline F-01 regression: an already-provisioned node refuses
    /// `seed.restore` with attacker-chosen words, and its identity is
    /// byte-identical afterwards. This test cannot pass without the gate.
    #[tokio::test]
    async fn provisioned_node_refuses_restore_and_identity_bytes_are_unchanged() {
        let (dir, auth) = fresh();
        let data_dir = dir.path();
        let identity_dir = data_dir.join("identity");

        // 1) The node's real identity, derived from seed A.
        let (_mnemonic_a, seed_a) = crate::seed::MasterSeed::generate().unwrap();
        crate::identity::NodeIdentity::from_seed(&identity_dir, &seed_a)
            .await
            .unwrap();
        let nostr_a = crate::seed::derive_node_nostr_key(&seed_a).unwrap();
        std::fs::write(
            identity_dir.join("nostr_secret"),
            nostr_a.secret_key().display_secret().to_string(),
        )
        .unwrap();

        // 2) The node is provisioned.
        auth.complete_onboarding().await.unwrap();

        // 3) Snapshot the key material an attacker would be trying to replace.
        let node_key_before = std::fs::read(identity_dir.join("node_key")).unwrap();
        let nostr_secret_before = std::fs::read(identity_dir.join("nostr_secret")).unwrap();

        // 4) The attack: a valid but attacker-chosen 24-word mnemonic, posted
        //    unauthenticated at seed.restore.
        let (attacker_mnemonic, _seed_b) = crate::seed::MasterSeed::generate().unwrap();
        let attacker_words: Vec<String> = attacker_mnemonic.words().map(str::to_string).collect();
        assert_eq!(attacker_words.len(), 24);

        let result = super::super::seed_rpc::restore_node_identity_from_words(
            data_dir,
            &auth,
            &attacker_words,
        )
        .await;

        assert!(
            result.is_err(),
            "a provisioned node must refuse seed.restore"
        );
        assert_eq!(
            std::fs::read(identity_dir.join("node_key")).unwrap(),
            node_key_before,
            "identity/node_key was overwritten by an unauthenticated caller (F-01)"
        );
        assert_eq!(
            std::fs::read(identity_dir.join("nostr_secret")).unwrap(),
            nostr_secret_before,
            "identity/nostr_secret was overwritten by an unauthenticated caller (F-01)"
        );
    }
}
