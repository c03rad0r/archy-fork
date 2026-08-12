//! Declarative, self-healing generation of app secrets.
//!
//! An app declares `generated_secrets` in its manifest; this module materialises
//! them just before `secret_env` is resolved. That keeps the migration's
//! data-driven bar: an app installs from its manifest alone — no host
//! provisioning and no per-app Rust — and every secret lands `0600`, owned by
//! the unprivileged (rootless) service user.
//!
//! Two properties make it safe to call on every install/reconcile tick:
//!
//! * **Idempotent** — a target file that already exists, is readable and
//!   non-empty is left untouched, so values are stable across ticks.
//! * **Self-healing without privilege** — a target file that exists but is
//!   *unreadable* (the classic `root:root`-owned secret left by some earlier
//!   path) is unlinked and rewritten. Unlinking needs write on the
//!   service-owned secrets dir, not on the file, so this recovers the broken
//!   state with no `chown` and no root — exactly what a rootless node needs.

use anyhow::{Context, Result};
use archipelago_container::{AppManifest, GeneratedSecret, SecretGenKind};
use rand::RngCore;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Plaintext-password length (bytes of entropy) for [`SecretGenKind::Bcrypt`].
const BCRYPT_PASSWORD_BYTES: usize = 24;

/// Materialise every declared generated secret for `manifest` under
/// `secrets_dir`. No-op when the manifest declares none. Safe to call on every
/// reconcile/install tick (idempotent + self-healing).
pub fn ensure_generated_secrets(secrets_dir: &Path, manifest: &AppManifest) -> Result<()> {
    let specs = &manifest.app.container.generated_secrets;
    if specs.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(secrets_dir)
        .with_context(|| format!("creating secrets dir {}", secrets_dir.display()))?;
    for gs in specs {
        ensure_one(secrets_dir, gs).with_context(|| format!("generating secret '{}'", gs.name))?;
    }
    Ok(())
}

fn ensure_one(dir: &Path, gs: &GeneratedSecret) -> Result<()> {
    let files = gs.target_files();

    // Idempotent fast path: every target file present, readable and non-empty.
    if files.iter().all(|f| readable_nonempty(&dir.join(f))) {
        return Ok(());
    }

    // Self-heal: drop any stale/unreadable target so the write below recreates
    // it owned by us. Unlinking uses the (service-owned) dir's write bit, so a
    // wrongly root-owned secret is recovered with no privilege escalation.
    for f in &files {
        let p = dir.join(f);
        if p.exists() && !readable_nonempty(&p) {
            tracing::warn!("regenerating unreadable/stale secret {}", p.display());
            fs::remove_file(&p)
                .with_context(|| format!("removing stale secret {}", p.display()))?;
        }
    }

    match gs.kind {
        SecretGenKind::Hex16 => write_secret(&dir.join(&gs.name), &random_hex(16))?,
        SecretGenKind::Hex32 => write_secret(&dir.join(&gs.name), &random_hex(32))?,
        SecretGenKind::Base64 => write_secret(&dir.join(&gs.name), &random_base64(32))?,
        SecretGenKind::Bcrypt => write_bcrypt_pair(dir, &gs.name)?,
    }
    Ok(())
}

/// Generate a fresh bcrypt credential pair for `name` under `dir`: the
/// server-facing hash at `<name>` and its plaintext sibling at `<name>.pw`,
/// both 0600 through the atomic [`write_secret`].
///
/// The single implementation of bcrypt generation on this platform —
/// [`ensure_one`]'s `Bcrypt` arm and
/// [`rotate_compromised_gateway_credential`] both go through here, so there is
/// one place where a credential comes into existence.
fn write_bcrypt_pair(dir: &Path, name: &str) -> Result<()> {
    let password = random_hex(BCRYPT_PASSWORD_BYTES);
    let hash = bcrypt::hash(&password, bcrypt::DEFAULT_COST)
        .context("bcrypt-hashing generated password")?;
    // Primary (server-facing hash) first, then the plaintext sibling.
    write_secret(&dir.join(name), &hash)?;
    write_secret(&dir.join(format!("{}.pw", name)), &password)?;
    Ok(())
}

/// True when `path` exists, is readable by this process, and is non-empty after
/// trimming. Any error (missing, permission denied, empty) reads as false.
fn readable_nonempty(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Fill `buf` from an explicitly named `OsRng`, guarded when it is long enough
/// for the degenerate predicate's false-positive bound to hold.
///
/// KEY-05 / F-10: these are the manifest-declared `generated_secrets` — app
/// passwords and API keys — and were the original F-10 finding. Every production
/// caller requests 16 or 32 bytes, so the guard is live in practice; the short
/// branch exists so a future caller asking for fewer cannot trip the guard's
/// length assertion, which is a programmer-error panic and not an input
/// condition.
fn fill_secret_bytes(buf: &mut [u8]) {
    if buf.len() >= crate::entropy::MIN_GUARDED_LEN {
        crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, buf).unwrap_or_else(|e| {
            panic!("refusing to generate an app secret from degenerate entropy: {e} (KEY-05)")
        });
    } else {
        rand::rngs::OsRng.fill_bytes(buf);
    }
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    fill_secret_bytes(&mut buf);
    hex::encode(buf)
}

/// `bytes` of entropy, standard base64 (with padding). For keys that a service
/// base64-decodes to recover the raw bytes (e.g. netbird's store encryptionKey).
fn random_base64(bytes: usize) -> String {
    use base64::Engine as _;
    let mut buf = vec![0u8; bytes];
    fill_secret_bytes(&mut buf);
    base64::engine::general_purpose::STANDARD.encode(buf)
}

/// Canonical secret name for the Fedimint gateway's admin bcrypt hash — must
/// match `generated_secrets: fedimint-gateway-hash` in
/// `apps/fedimint-gateway/manifest.yml` so the Rust orchestrator, first-boot
/// script, reconcile script and both deploy scripts all agree on one file
/// (FED-07: before this, scripts wrote `fedimint-gateway-password` while the
/// daemon read `fedimint-gateway-hash`).
pub const GATEWAY_HASH_SECRET_NAME: &str = "fedimint-gateway-hash";

/// Detection-only denylist of bcrypt hashes that shipped as hardcoded
/// fallback credentials in this repository before FED-07. `t9YjjxkiktrlYvjajB
/// /zgOMDnSNVg4HqrbDqh47u7Jf42whNdxNqC` was substituted for the Fedimint
/// gateway's admin password whenever the real per-install secret was
/// missing — in `config.rs`, `dependencies.rs`, and every shell install path
/// — meaning anyone holding a copy of this repo held the admin credential for
/// every gateway that ever took that fallback.
///
/// This value exists **only** so an install still carrying it can be
/// detected and rotated (plan 01-16 owns the migration). It must NEVER be
/// passed to a container, written to a fresh install, or handed back to a
/// caller by [`gateway_bcrypt_hash`] — that function returns `Err` instead.
/// This is the one and only place this value may appear in the tree.
const KNOWN_DEFAULT_GATEWAY_HASHES: &[&str] =
    &["$2y$10$t9YjjxkiktrlYvjajB/zgOMDnSNVg4HqrbDqh47u7Jf42whNdxNqC"];

/// Idempotently ensure the Fedimint gateway's admin credential exists under
/// `secrets_dir`: a fresh per-install bcrypt hash plus its `.pw` plaintext
/// sibling, both 0600. Delegates to [`ensure_one`] for the actual bcrypt
/// generation so there is exactly one implementation of that logic — this
/// also means a second call is a no-op (idempotent fast path) and a
/// present-but-unreadable file self-heals, so a reconcile tick never rotates
/// a working gateway credential out from under it.
pub fn ensure_gateway_credential(secrets_dir: &Path) -> Result<()> {
    fs::create_dir_all(secrets_dir)
        .with_context(|| format!("creating secrets dir {}", secrets_dir.display()))?;
    let gs = GeneratedSecret {
        name: GATEWAY_HASH_SECRET_NAME.to_string(),
        kind: SecretGenKind::Bcrypt,
    };
    ensure_one(secrets_dir, &gs)
}

/// Read the Fedimint gateway's canonical per-install bcrypt hash.
///
/// Returns `Err` naming the missing file when it is absent, empty, or
/// unreadable — callers must propagate that error rather than substitute a
/// literal, so an install with no credential fails loudly instead of quietly
/// starting an unauthenticated/default-credentialed gateway. Also returns
/// `Err` when the stored value matches [`KNOWN_DEFAULT_GATEWAY_HASHES`]: a
/// node carrying the shipped default must not be handed that value back by
/// this codebase, even to reconfigure itself with the same value it already
/// (insecurely) has.
pub fn gateway_bcrypt_hash(secrets_dir: &Path) -> Result<String> {
    let path = secrets_dir.join(GATEWAY_HASH_SECRET_NAME);
    let hash = fs::read_to_string(&path).with_context(|| {
        format!(
            "gateway credential missing at {} — call ensure_gateway_credential (or wait for the \
             next reconcile tick) to generate a per-install credential before starting the gateway",
            path.display()
        )
    })?;
    let hash = hash.trim();
    if hash.is_empty() {
        anyhow::bail!("gateway credential {} is empty", path.display());
    }
    if KNOWN_DEFAULT_GATEWAY_HASHES.contains(&hash) {
        anyhow::bail!(
            "gateway credential {} is a publicly known default that shipped hardcoded in this \
             repository before FED-07 — this install must rotate it (see plan 01-16) before the \
             gateway can be (re)configured",
            path.display()
        );
    }
    Ok(hash.to_string())
}

/// Detect and rotate a Fedimint gateway credential that is a publicly known
/// shipped default (FED-07 migration).
///
/// Returns `Ok(true)` only when the stored hash was an EXACT match for a
/// [`KNOWN_DEFAULT_GATEWAY_HASHES`] entry and has been replaced with a freshly
/// generated pair. An absent, unreadable, or simply unrecognised-but-unique
/// value returns `Ok(false)` and writes nothing: rotation must never fire on
/// "anything I did not generate this run", or an operator who deliberately set
/// their own credential would have it silently replaced.
///
/// Generating a credential where none exists is
/// [`ensure_gateway_credential`]'s job, not this function's.
///
/// **Rollback:** the replacement goes through [`write_secret`]'s atomic
/// temp-file-plus-rename, so a failure part-way through leaves the previous
/// credential file intact and the gateway keeps working with it. Do NOT
/// "improve" this into a truncate-in-place or a remove-then-write — that turns
/// a failed rotation into a gateway configured against a credential nobody
/// holds.
///
/// **Self-terminating:** the value written is freshly generated and therefore
/// not on the denylist, so the next reconcile tick detects nothing and changes
/// nothing. Rotation happens at most once per affected node.
pub fn rotate_compromised_gateway_credential(secrets_dir: &Path) -> Result<bool> {
    let path = secrets_dir.join(GATEWAY_HASH_SECRET_NAME);
    let Ok(current) = fs::read_to_string(&path) else {
        // Absent or unreadable: nothing to rotate. ensure_gateway_credential
        // owns materialising it.
        return Ok(false);
    };
    if !KNOWN_DEFAULT_GATEWAY_HASHES.contains(&current.trim()) {
        return Ok(false);
    }
    write_bcrypt_pair(secrets_dir, GATEWAY_HASH_SECRET_NAME).with_context(|| {
        format!(
            "rotating compromised gateway credential at {}",
            path.display()
        )
    })?;
    Ok(true)
}

/// Write an externally computed secret value (0600, atomic). For derived
/// secrets that aren't random generators — e.g. the btcpay internal-LND
/// connection string assembled in `container::lnd`.
pub(crate) fn write_secret_file(path: &Path, value: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("creating secrets dir {}", dir.display()))?;
    }
    write_secret(path, value)
}

/// Atomically write a `0600` secret: a temp file in the same dir (so the rename
/// is atomic), fsynced, then renamed over the target.
fn write_secret(path: &Path, value: &str) -> Result<()> {
    let dir = path
        .parent()
        .context("secret path has no parent directory")?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("secret path has no filename")?;
    let tmp = dir.join(format!(".{name}.tmp"));

    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .with_context(|| format!("creating temp secret {}", tmp.display()))?;
    f.write_all(value.as_bytes())
        .with_context(|| format!("writing temp secret {}", tmp.display()))?;
    f.sync_all()
        .with_context(|| format!("fsync temp secret {}", tmp.display()))?;
    drop(f);

    fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use archipelago_container::SecretGenKind;
    use std::os::unix::fs::PermissionsExt;

    fn manifest_with(secrets: Vec<GeneratedSecret>) -> AppManifest {
        let mut m: AppManifest = serde_yaml::from_str(
            "app:\n  id: t\n  name: t\n  version: 1.0.0\n  container:\n    image: x:y\n",
        )
        .unwrap();
        m.app.container.generated_secrets = secrets;
        m
    }

    fn gs(name: &str, kind: SecretGenKind) -> GeneratedSecret {
        GeneratedSecret {
            name: name.to_string(),
            kind,
        }
    }

    #[test]
    fn generates_hex_and_bcrypt_with_0600() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest_with(vec![
            gs("tok", SecretGenKind::Hex16),
            gs("admin", SecretGenKind::Bcrypt),
        ]);
        ensure_generated_secrets(dir.path(), &m).unwrap();

        let tok = std::fs::read_to_string(dir.path().join("tok")).unwrap();
        assert_eq!(tok.trim().len(), 32, "hex16 = 16 bytes = 32 hex chars");

        let hash = std::fs::read_to_string(dir.path().join("admin")).unwrap();
        let pw = std::fs::read_to_string(dir.path().join("admin.pw")).unwrap();
        assert!(hash.starts_with("$2"), "bcrypt hash shape");
        assert!(
            bcrypt::verify(pw.trim(), hash.trim()).unwrap(),
            "pw matches hash"
        );

        for f in ["tok", "admin", "admin.pw"] {
            let mode = std::fs::metadata(dir.path().join(f))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{f} must be 0600");
        }
    }

    #[test]
    fn idempotent_value_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest_with(vec![gs("tok", SecretGenKind::Hex32)]);
        ensure_generated_secrets(dir.path(), &m).unwrap();
        let first = std::fs::read_to_string(dir.path().join("tok")).unwrap();
        ensure_generated_secrets(dir.path(), &m).unwrap();
        let second = std::fs::read_to_string(dir.path().join("tok")).unwrap();
        assert_eq!(
            first, second,
            "a present readable secret is never rewritten"
        );
    }

    #[test]
    fn gateway_credential_fresh_generation_verifies_and_is_0600() {
        let dir = tempfile::tempdir().unwrap();
        ensure_gateway_credential(dir.path()).unwrap();

        let hash = std::fs::read_to_string(dir.path().join(GATEWAY_HASH_SECRET_NAME)).unwrap();
        let pw = std::fs::read_to_string(dir.path().join(format!("{GATEWAY_HASH_SECRET_NAME}.pw")))
            .unwrap();
        assert!(bcrypt::verify(pw.trim(), hash.trim()).unwrap());

        for f in [
            GATEWAY_HASH_SECRET_NAME.to_string(),
            format!("{GATEWAY_HASH_SECRET_NAME}.pw"),
        ] {
            let mode = std::fs::metadata(dir.path().join(&f))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{f} must be 0600");
        }

        let read_back = gateway_bcrypt_hash(dir.path()).unwrap();
        assert_eq!(read_back, hash.trim());
    }

    #[test]
    fn gateway_credential_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        ensure_gateway_credential(dir.path()).unwrap();
        let first = gateway_bcrypt_hash(dir.path()).unwrap();
        ensure_gateway_credential(dir.path()).unwrap();
        let second = gateway_bcrypt_hash(dir.path()).unwrap();
        assert_eq!(first, second, "second call must not rotate the credential");
    }

    #[test]
    fn gateway_credential_missing_is_a_named_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = gateway_bcrypt_hash(dir.path()).unwrap_err();
        assert!(
            err.to_string().contains(GATEWAY_HASH_SECRET_NAME),
            "error must name the missing secret file: {err}"
        );
    }

    #[test]
    fn gateway_credential_rejects_known_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(GATEWAY_HASH_SECRET_NAME),
            KNOWN_DEFAULT_GATEWAY_HASHES[0],
        )
        .unwrap();
        let err = gateway_bcrypt_hash(dir.path()).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("default"),
            "error must explain the denylisted value: {err}"
        );
    }

    #[test]
    fn gateway_credential_is_per_install_not_per_build() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        ensure_gateway_credential(dir_a.path()).unwrap();
        ensure_gateway_credential(dir_b.path()).unwrap();
        let hash_a = gateway_bcrypt_hash(dir_a.path()).unwrap();
        let hash_b = gateway_bcrypt_hash(dir_b.path()).unwrap();
        assert_ne!(hash_a, hash_b, "two fresh installs must not share a hash");
    }

    // ── FED-07 migration: rotating a shipped default off an existing node ──

    #[test]
    fn rotates_a_denylisted_gateway_credential() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(GATEWAY_HASH_SECRET_NAME),
            KNOWN_DEFAULT_GATEWAY_HASHES[0],
        )
        .unwrap();

        assert!(rotate_compromised_gateway_credential(dir.path()).unwrap());

        // The new value is readable through the normal accessor, which means
        // it is neither empty nor still denylisted.
        let rotated = gateway_bcrypt_hash(dir.path()).unwrap();
        assert!(!KNOWN_DEFAULT_GATEWAY_HASHES.contains(&rotated.as_str()));

        // The plaintext sibling was written too and verifies against the hash,
        // so the operator can actually get back into the gateway.
        let pw = std::fs::read_to_string(dir.path().join(format!("{GATEWAY_HASH_SECRET_NAME}.pw")))
            .unwrap();
        assert!(bcrypt::verify(pw.trim(), rotated.trim()).unwrap());

        for f in [
            GATEWAY_HASH_SECRET_NAME.to_string(),
            format!("{GATEWAY_HASH_SECRET_NAME}.pw"),
        ] {
            let mode = std::fs::metadata(dir.path().join(&f))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{f} must stay 0600 after rotation");
        }
    }

    #[test]
    fn leaves_a_unique_gateway_credential_alone() {
        let dir = tempfile::tempdir().unwrap();
        ensure_gateway_credential(dir.path()).unwrap();
        let before = gateway_bcrypt_hash(dir.path()).unwrap();

        assert!(!rotate_compromised_gateway_credential(dir.path()).unwrap());
        assert_eq!(before, gateway_bcrypt_hash(dir.path()).unwrap());
    }

    #[test]
    fn leaves_an_unrecognised_credential_alone() {
        // The adjacency edge that matters: an operator's own hand-set value is
        // not on the denylist and must survive. Rotation is denylist-exact,
        // never "anything I did not generate".
        let dir = tempfile::tempdir().unwrap();
        let operator_set = "$2y$10$operatorChosenValueThatWeMustNeverTouchAAAAAAAAAAAAAAAAAAAAA";
        std::fs::write(dir.path().join(GATEWAY_HASH_SECRET_NAME), operator_set).unwrap();

        assert!(!rotate_compromised_gateway_credential(dir.path()).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(GATEWAY_HASH_SECRET_NAME)).unwrap(),
            operator_set
        );
    }

    #[test]
    fn no_op_when_no_gateway_credential_exists() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!rotate_compromised_gateway_credential(dir.path()).unwrap());
        assert!(!dir.path().join(GATEWAY_HASH_SECRET_NAME).exists());
    }

    #[test]
    fn rotation_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(GATEWAY_HASH_SECRET_NAME),
            KNOWN_DEFAULT_GATEWAY_HASHES[0],
        )
        .unwrap();

        assert!(rotate_compromised_gateway_credential(dir.path()).unwrap());
        let after_first = gateway_bcrypt_hash(dir.path()).unwrap();

        // Second tick: nothing detected, nothing changed. This is what stops a
        // reconcile loop from recreating the gateway on every pass.
        assert!(!rotate_compromised_gateway_credential(dir.path()).unwrap());
        assert_eq!(after_first, gateway_bcrypt_hash(dir.path()).unwrap());
    }

    #[test]
    fn rotation_touches_no_other_secret() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(GATEWAY_HASH_SECRET_NAME),
            KNOWN_DEFAULT_GATEWAY_HASHES[0],
        )
        .unwrap();
        let bystanders = [
            ("mempool-db-password", "mempool-value"),
            ("immich-db-password", "immich-value"),
            ("fmcd-password", "fmcd-value"),
            ("bitcoin-rpc-password", "bitcoin-value"),
        ];
        for (name, value) in bystanders {
            std::fs::write(dir.path().join(name), value).unwrap();
        }

        assert!(rotate_compromised_gateway_credential(dir.path()).unwrap());

        for (name, value) in bystanders {
            assert_eq!(
                std::fs::read_to_string(dir.path().join(name)).unwrap(),
                value,
                "{name} must be byte-identical after a gateway rotation"
            );
        }
    }

    #[test]
    fn self_heals_unreadable_secret() {
        // Simulate the root-owned case: a present-but-unreadable file. We can't
        // chmod-away read as the owner in a unit test, so emulate "unreadable"
        // via the empty-file branch (readable_nonempty == false), which drives
        // the same unlink+regenerate path.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tok"), "").unwrap();
        let m = manifest_with(vec![gs("tok", SecretGenKind::Hex16)]);
        ensure_generated_secrets(dir.path(), &m).unwrap();
        let v = std::fs::read_to_string(dir.path().join("tok")).unwrap();
        assert_eq!(v.trim().len(), 32, "stale/empty secret was regenerated");
    }
}
