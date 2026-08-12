//! Container image signature verification (cosign).
//!
//! The manifest/catalog `image_signature` field is a *claim* that the image
//! is signed with the fleet's cosign key. Verification runs at the pull
//! choke points (`PodmanClient::pull_image`, `DockerRuntime::pull_image`);
//! a declared signature that cannot be verified hard-fails the pull.
//!
//! Every manifest has carried the literal placeholder `cosign://...` since
//! the field was introduced — that means "not signed yet" and is treated as
//! no claim, so enforcement stays dormant until the signing ceremony
//! publishes real signatures AND nodes carry the pinned cosign public key.
//! Ship order matters: key + cosign binary reach the fleet first, real
//! signature values in the catalog come after.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

/// The literal placeholder every pre-ceremony manifest carries.
pub const SIGNATURE_PLACEHOLDER: &str = "cosign://...";

/// Env override for the pinned cosign public key path (tests, staging).
pub const COSIGN_PUBKEY_ENV: &str = "ARCHIPELAGO_COSIGN_PUBKEY";

const DEFAULT_PUBKEY_PATH: &str = "/etc/archipelago/cosign.pub";

const COSIGN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureClaim {
    /// No signature declared (field absent or empty).
    None,
    /// The literal `cosign://...` placeholder — manifest predates real signing.
    Placeholder,
    /// A real declared signature reference; MUST verify or the pull fails.
    Declared(String),
}

pub fn classify_signature(signature: Option<&str>) -> SignatureClaim {
    match signature.map(str::trim) {
        None | Some("") => SignatureClaim::None,
        Some(SIGNATURE_PLACEHOLDER) => SignatureClaim::Placeholder,
        Some(s) => SignatureClaim::Declared(s.to_string()),
    }
}

fn pinned_pubkey_path() -> PathBuf {
    std::env::var(COSIGN_PUBKEY_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_PUBKEY_PATH))
}

/// Verify a declared image signature with the fleet's pinned cosign key.
/// Any failure — missing key, missing cosign binary, verification error —
/// is a hard error: an image that CLAIMS to be signed must never be pulled
/// on a node that can't prove the claim.
pub async fn verify_declared_signature(
    image: &str,
    sig_ref: &str,
    allow_insecure_registry: bool,
) -> Result<()> {
    verify_with_key_path(
        image,
        sig_ref,
        &pinned_pubkey_path(),
        allow_insecure_registry,
    )
    .await
}

async fn verify_with_key_path(
    image: &str,
    sig_ref: &str,
    key_path: &std::path::Path,
    allow_insecure_registry: bool,
) -> Result<()> {
    if !key_path.exists() {
        bail!(
            "Image '{image}' declares signature '{sig_ref}' but the pinned cosign \
             public key is missing at {} (override with {COSIGN_PUBKEY_ENV}). \
             Refusing to pull an image whose signature claim cannot be verified.",
            key_path.display()
        );
    }

    // Self-managed key => signatures aren't in the public Rekor transparency
    // log, so tlog verification must be disabled explicitly (cosign v2
    // defaults it on and would fail every private-key signature otherwise).
    let mut cmd = tokio::process::Command::new("cosign");
    cmd.arg("verify")
        .arg("--key")
        .arg(key_path)
        .arg("--insecure-ignore-tlog=true");
    if allow_insecure_registry {
        // podman's --tls-verify=false covers both plain HTTP and bad TLS;
        // cosign splits those into two flags — pass both to match.
        cmd.arg("--allow-insecure-registry");
        cmd.arg("--allow-http-registry");
    }
    cmd.arg(image);

    let output = tokio::time::timeout(COSIGN_TIMEOUT, cmd.output())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "cosign verify timed out after {}s for image '{image}'",
                COSIGN_TIMEOUT.as_secs()
            )
        })?
        .with_context(|| {
            format!(
                "Failed to run cosign for image '{image}' which declares signature \
                 '{sig_ref}' — is cosign installed? A declared signature cannot be \
                 skipped."
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Signature verification FAILED for image '{image}' (declared: '{sig_ref}'): \
             {stderr}"
        );
    }

    tracing::info!("cosign signature verified for image {image}");
    Ok(())
}

/// Shared pull-site gate: decide whether the pull may proceed.
/// Returns Ok(()) for unsigned/placeholder claims (with a log line) and only
/// after successful cosign verification for declared ones.
pub async fn enforce_signature_claim(
    image: &str,
    signature: Option<&str>,
    allow_insecure_registry: bool,
) -> Result<()> {
    match classify_signature(signature) {
        SignatureClaim::None => {
            tracing::debug!("image {image}: no signature declared, pulling unverified");
            Ok(())
        }
        SignatureClaim::Placeholder => {
            tracing::debug!(
                "image {image}: signature is the pre-ceremony placeholder, pulling unverified"
            );
            Ok(())
        }
        SignatureClaim::Declared(sig_ref) => {
            verify_declared_signature(image, &sig_ref, allow_insecure_registry).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_and_empty_signatures_are_no_claim() {
        assert_eq!(classify_signature(None), SignatureClaim::None);
        assert_eq!(classify_signature(Some("")), SignatureClaim::None);
        assert_eq!(classify_signature(Some("   ")), SignatureClaim::None);
    }

    #[test]
    fn literal_placeholder_is_not_a_claim() {
        assert_eq!(
            classify_signature(Some("cosign://...")),
            SignatureClaim::Placeholder
        );
        assert_eq!(
            classify_signature(Some("  cosign://...  ")),
            SignatureClaim::Placeholder
        );
    }

    #[test]
    fn real_values_are_declared_claims() {
        assert_eq!(
            classify_signature(Some("cosign://sha256-abc.sig")),
            SignatureClaim::Declared("cosign://sha256-abc.sig".to_string())
        );
        // Unknown schemes still count as a claim — better to fail closed on
        // a value we don't understand than to pull unverified.
        assert_eq!(
            classify_signature(Some("sigstore://whatever")),
            SignatureClaim::Declared("sigstore://whatever".to_string())
        );
    }

    #[tokio::test]
    async fn declared_signature_without_pinned_key_hard_fails() {
        let err = verify_with_key_path(
            "registry.example/app:1.0",
            "cosign://sha256-abc.sig",
            std::path::Path::new("/nonexistent/cosign.pub"),
            false,
        )
        .await
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("pinned cosign public key is missing"));
    }

    #[tokio::test]
    async fn unsigned_and_placeholder_claims_pass_the_gate() {
        enforce_signature_claim("registry.example/app:1.0", None, false)
            .await
            .unwrap();
        enforce_signature_claim("registry.example/app:1.0", Some("cosign://..."), false)
            .await
            .unwrap();
    }
}
