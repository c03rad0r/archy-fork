mod handlers;

use anyhow::Result;

pub(super) fn validate_identity_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 128 {
        anyhow::bail!("Invalid identity id: must be 1-128 characters");
    }
    if id.contains("..") || id.contains('/') || id.contains('\\') || id.contains('\0') {
        anyhow::bail!("Invalid identity id: contains forbidden characters");
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':')
    {
        anyhow::bail!("Invalid identity id: must be alphanumeric, hyphens, underscores, or colons");
    }
    Ok(())
}
