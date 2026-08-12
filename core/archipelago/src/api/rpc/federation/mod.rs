mod handlers;

use anyhow::Result;

pub(super) fn validate_did(did: &str) -> Result<()> {
    if did.is_empty() || did.len() > 256 {
        anyhow::bail!("Invalid DID: must be 1-256 characters");
    }
    if !did.starts_with("did:") {
        anyhow::bail!("Invalid DID: must start with 'did:'");
    }
    if did.contains("..") || did.contains('/') || did.contains('\\') || did.contains('\0') {
        anyhow::bail!("Invalid DID: contains forbidden characters");
    }
    Ok(())
}
