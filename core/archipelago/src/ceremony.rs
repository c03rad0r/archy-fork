//! Release-root signing ceremony — the publisher-side counterpart to
//! `trust::anchor`. Run as a subcommand of the same binary so it reuses the
//! exact key derivation (`seed::derive_release_root_ed25519`) and canonical
//! signing (`trust::signed_doc::sign_detached`) the fleet verifies against.
//!
//! Usage (the mnemonic is read from the `RELEASE_MASTER_MNEMONIC` env var or
//! stdin — never an argv so it stays out of shell history / `ps`):
//!
//! ```text
//!   archipelago ceremony gen
//!       Generate a fresh 24-word release master mnemonic and print it plus the
//!       derived release-root pubkey + did. Back the mnemonic up OFFLINE.
//!
//!   RELEASE_MASTER_MNEMONIC="word1 …" archipelago ceremony pubkey
//!       Print the release-root pubkey hex (for ARCHY_RELEASE_ROOT_PUBKEY /
//!       trust::anchor::RELEASE_ROOT_PUBKEY_HEX) and the signer did:key.
//!
//!   RELEASE_MASTER_MNEMONIC="word1 …" archipelago ceremony sign <file.json>
//!       Sign a JSON document (e.g. releases/app-catalog.json) in place: insert
//!       `signature` + `signed_by` over the canonical form, matching exactly
//!       what `trust::verify_detached` recomputes on every node.
//!
//!   archipelago ceremony verify <file.json>
//!       Verify a signed JSON document against the compiled-in release-root
//!       anchor. Exits non-zero unless the signature verifies AND the signer
//!       is the pinned anchor. Needs no mnemonic — used as the publish gate.
//! ```

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;

use crate::seed::{self, MasterSeed};
use crate::trust::{did, signed_doc};

const ENV_MNEMONIC: &str = "RELEASE_MASTER_MNEMONIC";

/// True if argv selects the ceremony subcommand. Checked before any server init.
pub fn is_ceremony_invocation() -> bool {
    std::env::args().nth(1).as_deref() == Some("ceremony")
}

/// Entry point for `archipelago ceremony …`. Returns Ok(()) on success; the
/// caller (main) should exit without starting the server.
pub fn run() -> Result<()> {
    let sub = std::env::args().nth(2).unwrap_or_default();
    match sub.as_str() {
        "gen" => cmd_gen(),
        "pubkey" => cmd_pubkey(),
        "sign" => {
            let file = std::env::args()
                .nth(3)
                .context("usage: archipelago ceremony sign <file.json>")?;
            cmd_sign(&file)
        }
        "verify" => {
            let file = std::env::args()
                .nth(3)
                .context("usage: archipelago ceremony verify <file.json>")?;
            cmd_verify(&file)
        }
        other => {
            bail!(
                "unknown ceremony subcommand {:?}; expected gen | pubkey | sign <file> | verify <file>",
                other
            )
        }
    }
}

fn cmd_gen() -> Result<()> {
    let (mnemonic, seed) = MasterSeed::generate().context("generate mnemonic")?;
    let key = seed::derive_release_root_ed25519(&seed).context("derive release-root")?;
    eprintln!("⚠  Back this mnemonic up OFFLINE. It is the ONLY way to re-derive");
    eprintln!("   the release-root signing key. Anyone with it can sign for the fleet.\n");
    println!("RELEASE_MASTER_MNEMONIC=\"{}\"", mnemonic);
    print_key(&key);
    Ok(())
}

fn cmd_pubkey() -> Result<()> {
    let key = load_release_root_key()?;
    print_key(&key);
    Ok(())
}

fn cmd_sign(path: &str) -> Result<()> {
    let key = load_release_root_key()?;

    let body = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&body).with_context(|| format!("parse {path} as JSON"))?;
    {
        let obj = value
            .as_object_mut()
            .context("document root must be a JSON object")?;
        // Re-sign cleanly: drop any prior signature so the preimage matches.
        obj.remove("signature");
        obj.remove("signed_by");
    }

    let (signature, signed_by) =
        signed_doc::sign_detached(&key, &value).context("sign document")?;

    let obj = value.as_object_mut().expect("checked above");
    obj.insert("signature".into(), serde_json::Value::String(signature));
    obj.insert(
        "signed_by".into(),
        serde_json::Value::String(signed_by.clone()),
    );

    let pretty = serde_json::to_string_pretty(&value).context("serialize signed document")?;
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, format!("{pretty}\n")).with_context(|| format!("write {tmp}"))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename {tmp} -> {path}"))?;

    eprintln!("✓ signed {path}");
    eprintln!("  signed_by: {signed_by}");
    Ok(())
}

fn cmd_verify(path: &str) -> Result<()> {
    let body = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&body).with_context(|| format!("parse {path} as JSON"))?;
    match signed_doc::verify_detached(&value)? {
        signed_doc::SignatureStatus::Verified {
            signer_did,
            anchored: true,
        } => {
            eprintln!("✓ {path} verified — signed by the pinned release root");
            eprintln!("  signed_by: {signer_did}");
            Ok(())
        }
        signed_doc::SignatureStatus::Verified {
            signer_did,
            anchored: false,
        } => {
            // Only reachable if no anchor is compiled in/overridden — the
            // signature is self-consistent but proves nothing about identity.
            bail!("{path} signed by {signer_did}, but no release-root anchor is pinned to compare against")
        }
        signed_doc::SignatureStatus::Unsigned => {
            bail!("{path} is NOT signed (no `signature` field)")
        }
    }
}

/// Derive the release-root signing key from the mnemonic in env/stdin.
fn load_release_root_key() -> Result<SigningKey> {
    let phrase = read_mnemonic()?;
    let (_mnemonic, seed) = MasterSeed::from_mnemonic_words(phrase.trim())
        .context("invalid release master mnemonic")?;
    seed::derive_release_root_ed25519(&seed).context("derive release-root")
}

/// Read the mnemonic from `RELEASE_MASTER_MNEMONIC` or, if unset, stdin.
fn read_mnemonic() -> Result<String> {
    if let Ok(v) = std::env::var(ENV_MNEMONIC) {
        if !v.trim().is_empty() {
            return Ok(v);
        }
    }
    use std::io::Read;
    eprintln!("Paste the release master mnemonic, then Ctrl-D:");
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("read mnemonic from stdin")?;
    if buf.trim().is_empty() {
        bail!("no mnemonic provided (set {ENV_MNEMONIC} or pipe it on stdin)");
    }
    Ok(buf)
}

fn print_key(key: &SigningKey) {
    let vk = key.verifying_key();
    println!("RELEASE_ROOT_PUBKEY_HEX={}", hex::encode(vk.to_bytes()));
    println!("signed_by_did={}", did::did_key_for_ed25519(&vk));
}
