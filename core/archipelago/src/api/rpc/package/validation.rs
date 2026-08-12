use anyhow::Result;

/// Validate that a package/app ID is safe (lowercase alphanumeric + hyphens, 1-64 chars).
pub(in crate::api::rpc) fn validate_app_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 64 {
        anyhow::bail!("Invalid app id: must be 1-64 characters");
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        anyhow::bail!("Invalid app id: only lowercase letters, digits, and hyphens allowed");
    }
    if id.starts_with('-') {
        anyhow::bail!("Invalid app id: must not start with a hyphen");
    }
    Ok(())
}
