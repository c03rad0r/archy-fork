//! Trusted-registry policy for container image references — the single
//! source of truth. The RPC boundary (`api::rpc::package::config`) and the
//! orchestrator's pull sites both validate against this, so a catalog- or
//! manifest-supplied ref can't reach `pull_image` unchecked (§A of the
//! 1.8.0 hardening plan).

/// The registry's previous address, before it moved behind a domain.
///
/// TRANSITIONAL — remove once the app catalog has been regenerated and
/// re-signed against `source.archipelago-foundation.org`. The catalog is a
/// signed artifact, so its image refs cannot be rewritten in place without
/// invalidating the signature; until the signing ceremony runs, deployed
/// nodes still resolve every app through a catalog that names this host.
/// Dropping it from the trusted list before then makes each catalog-driven
/// install fail with "not from a trusted registry".
pub const LEGACY_REGISTRY_HOST: &str = "146.59.87.168:3000";

/// Registries images may be pulled from with an explicit host part.
/// (git.tx1138.com was removed 2026-07-10: the host is retired and must
/// never be pulled through again.)
pub const TRUSTED_REGISTRIES: &[&str] = &[
    "docker.io",
    "ghcr.io",
    "localhost",
    "source.archipelago-foundation.org",
    LEGACY_REGISTRY_HOST,
];

/// Validate a container image reference.
///
/// Accepts:
///   * refs whose explicit registry host is on [`TRUSTED_REGISTRIES`]
///     (`docker.io/grafana/grafana`, `source.archipelago-foundation.org/archy/x:1`), and
///   * registry-less Docker Hub shorthand (`nginx`, `grafana/grafana`) —
///     the first segment has no `.`/`:` so it cannot name an attacker host;
///     resolution follows the host's registries.conf search order.
///
/// Rejects empty/oversized refs, shell metacharacters, and any ref whose
/// explicit registry host is not on the allowlist.
pub fn is_valid_docker_image(image: &str) -> bool {
    if image.is_empty() || image.len() > 256 {
        return false;
    }
    // Reject shell metacharacters
    let dangerous_chars = [
        '&', '|', ';', '`', '$', '(', ')', '<', '>', '\n', '\r', ' ', '\t',
    ];
    if image.chars().any(|c| dangerous_chars.contains(&c)) {
        return false;
    }
    let first_segment = match image.split('/').next() {
        Some(r) if !r.is_empty() => r,
        _ => return false,
    };
    if TRUSTED_REGISTRIES.contains(&first_segment) {
        return true;
    }
    // No dot/colon in the first segment ⇒ it's a Docker Hub namespace or a
    // bare repo name, not a registry host — allowed. Anything that *looks*
    // like a host (has a dot or port) but isn't allowlisted is rejected.
    !first_segment.contains('.') && !first_segment.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_trusted_registries() {
        for img in [
            "docker.io/library/nginx:1.25",
            "ghcr.io/owner/app:latest",
            "localhost/archy-dev:1",
            "source.archipelago-foundation.org/archy/bitcoin-knots:28.1",
        ] {
            assert!(is_valid_docker_image(img), "{img} should be accepted");
        }
    }

    #[test]
    fn rejects_retired_tx1138_registry() {
        // Retired 2026-07-10 — refs through the dead host must be refused
        // at the pull site, not time out against it.
        assert!(!is_valid_docker_image("git.tx1138.com/lfg2025/x:2"));
    }

    #[test]
    fn accepts_docker_hub_shorthand() {
        for img in ["nginx", "grafana/grafana:11.2.0", "lightninglabs/lnd:v0.18"] {
            assert!(is_valid_docker_image(img), "{img} should be accepted");
        }
    }

    #[test]
    fn rejects_untrusted_registry_hosts() {
        for img in [
            "evil.com/backdoor:latest",
            "203.0.113.7:5000/x",
            "registry.gitlab.com/x/y",
            "quay.io/x/y",
        ] {
            assert!(!is_valid_docker_image(img), "{img} should be rejected");
        }
    }

    #[test]
    fn rejects_malformed_refs() {
        assert!(!is_valid_docker_image(""));
        assert!(!is_valid_docker_image(&"a".repeat(257)));
        assert!(!is_valid_docker_image("docker.io/x; rm -rf /"));
        assert!(!is_valid_docker_image("docker.io/$(curl evil)"));
        assert!(!is_valid_docker_image("/leading-slash"));
    }
}
