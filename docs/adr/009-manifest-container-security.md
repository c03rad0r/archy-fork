# ADR-009: Manifest-Level Container Security Enforcement

## Status

Accepted

## Context

Archipelago runs third-party applications as containers. Without enforcement, containers could:

- Run as root and escalate privileges
- Access the host filesystem
- Modify their own binaries (persistence of malicious code)
- Acquire unnecessary Linux capabilities
- Use unverified or tampered container images

Other node OS projects (Umbrel, Start9) vary in their security enforcement. Archipelago targets a higher security bar suitable for handling Bitcoin private keys and personal data.

## Decision

Enforce security constraints at the **manifest level**, applied automatically during container creation. Every container MUST comply with these non-negotiable defaults:

### Mandatory Security Defaults

| Constraint | Value | Rationale |
|-----------|-------|-----------|
| `readonly_root` | `true` | Prevents runtime filesystem modification (anti-persistence) |
| `no_new_privileges` | `true` | Prevents privilege escalation via setuid/setgid |
| `user` | UID > 1000 | Never run as root |
| `capabilities` | Drop ALL, add only required | Principle of least privilege |
| `image_tag` | Pinned version | No `latest` tags — reproducible deploys |
| `seccomp_profile` | Default | Blocks dangerous syscalls |

### Manifest Enforcement

The `core/container/` module validates manifests before container creation:

1. **Parse** the YAML manifest
2. **Validate** all required security fields are present
3. **Reject** manifests that violate mandatory defaults (e.g., `readonly_root: false` without explicit override)
4. **Apply** security context during `podman create`

### Optional Overrides

Some apps legitimately need elevated privileges:

- `readonly_root: false` — Only for apps that must write to their root filesystem (documented reason required)
- Additional capabilities (e.g., `NET_ADMIN` for VPN apps) — must be explicitly listed and justified

## Consequences

### Positive

- Defense in depth — even if a container image is compromised, damage is limited
- Consistent security posture across all apps
- Transparent — users can inspect any app's security manifest
- Aligns with industry best practices (CIS Benchmarks, NIST)

### Negative

- Some apps may not work without modifications (e.g., apps expecting root)
- Read-only root requires explicit volume mounts for writable directories
- Developers must understand and comply with the security model
- Slightly more complex manifest format than competitors

### Mitigations

- Clear documentation in `docs/app-manifest-spec.md`
- Example manifests for common app patterns
- Build-time validation catches issues before deployment
- Override mechanism for legitimate exceptions (with audit trail)

## Implementation status

The decision above stands; this section records how much of it is actually
enforced today, because the "non-negotiable" table overstates it. Verified
against `core/container/src/manifest.rs` and `core/security/src/`:

| Constraint | Reality |
|---|---|
| `capabilities` drop-all + allow-list | ✅ **Enforced.** A capability outside the nine-entry allow-list is a parse error, so the app cannot install |
| Bind-mount confinement | ✅ **Enforced** (stronger than this ADR describes): sources must be under `/var/lib/archipelago/`, a named volume, or one of two reviewed exceptions |
| `readonly_root` / `no_new_privileges` | ◐ **Defaults, not gates.** Both default to `true` when omitted, but `validate_security()` does not reject an explicit `false` — the "reject manifests that violate mandatory defaults" step does not exist |
| `image_tag` pinned | ◐ **Preflight only.** `scripts/validate-app-manifest.sh` grades it; the parser accepts `:latest` and the app installs |
| `user` UID > 1000 | ❌ **Not validated.** The runtime manifest parser has no UID check at all (the marketplace schema has an advisory one, which is a different type) |
| `seccomp_profile` | ❌ **Does not exist.** The string `seccomp` appears nowhere in `core/` — not as code, not as a TODO |
| AppArmor | ❌ **Inert.** `container_policies.rs` can generate and `apparmor_parser -r` a profile, but its own comment says `TODO: Configure Podman to use the profile`. `security.apparmor_profile` is parsed into a manifest field that nothing ever reads |

So the accurate summary is: capability and mount confinement are hard gates,
the process-hardening flags are safe-by-default rather than enforced, and the
kernel-level sandboxing (seccomp/AppArmor) named in the decision was never
wired up. Closing the last two rows is tracked in
[`1.8.0-RELEASE-HARDENING-PLAN.md`](../1.8.0-RELEASE-HARDENING-PLAN.md).

## References

- `docs/app-manifest-spec.md` — Full manifest specification
- `core/container/src/` — Container security implementation
- `core/security/src/` — AppArmor profiles and secrets management
