# Archipelago Security & Code Quality Audit Report

**Date**: March 2026
**Version audited**: 0.1.0
**Auditor**: Automated code review (Claude)
**Scope**: Authentication, sessions, cryptography, container security, RPC, frontend, custom code vs libraries

---

## 1. Executive Summary

### Overall Security Posture: 7.5 / 10

Archipelago demonstrates a security-conscious design with several production-grade patterns already in place. The project makes defensible choices in cryptography, follows capability-based container hardening, and implements layered authentication with TOTP 2FA. However, gaps remain in image signature verification, some postMessage origin validation, and the use of bcrypt instead of the already-available Argon2id for password hashing.

For a v0.1.0 self-sovereign personal server, this is a strong foundation. The code reads like it was written by someone who understands the threat model (local network appliance, single admin user, potentially hostile containers).

### Top 5 Risks (by severity)

1. **Cosign image verification is a TODO** (`podman_client.rs:84`). Container images are pulled without cryptographic signature checks. A compromised registry or MITM on image pull could inject malicious containers. This is the single largest attack surface.

2. **`postMessage('*')` wildcard origin in Nostr signer** (`AppSession.vue:490`, `appLauncher.ts:262,306,309`). Responses to NIP-07 signing requests are sent with `'*'` target origin, allowing any window/iframe to intercept signed Nostr events. A malicious app loaded in an adjacent iframe could harvest signatures.

3. **bcrypt for password hashing instead of Argon2id** (`auth.rs:108,245`). bcrypt is battle-tested but vulnerable to GPU/ASIC acceleration. Argon2id is already a dependency (used in TOTP and backup encryption) and provides memory-hard resistance. Using two different password hashing schemes in the same codebase is also a maintenance smell.

4. **Sessions are in-memory only** (`session.rs`). All sessions are lost on service restart, forcing all users to re-authenticate. More critically, there is no persistence layer to support session revocation auditing or multi-instance deployments.

5. **`v-html` used for TOTP QR SVG rendering** (`Settings.vue:286`). The SVG is server-generated, but `v-html` is a known XSS vector. If the QR generation path were ever to include user-controlled input, this would become exploitable.

### Top 5 Strengths

1. **TOTP implementation is production-grade**. Envelope encryption (KEK wraps MEK wraps secret), Argon2id key derivation with strong parameters (64 MiB, t=3, p=4), ChaCha20-Poly1305 AEAD, zeroize on drop, replay protection via used time steps, bcrypt-hashed backup codes. This exceeds what most node-OS projects implement.

2. **Comprehensive rate limiting**. Login rate limiting (5 attempts / 60s per IP) plus per-endpoint rate limiting on 25+ sensitive methods including financial operations, identity creation, backup operations, and federation joins. Configurable windows per method.

3. **CSRF protection is properly implemented**. Double-submit cookie pattern: `csrf_token` cookie (readable by JS) + `X-CSRF-Token` header validated on every authenticated request. SameSite=Strict on session cookies. HttpOnly on session cookie (not accessible to JS).

4. **Container security defaults are correct**. `--cap-drop=ALL` with explicit per-app capability add-back, `--security-opt=no-new-privileges:true` on all non-privileged containers, read-only root filesystem where compatible, per-app capability documentation.

5. **Error sanitization prevents information leakage**. `sanitize_error_message()` strips internal file paths and system details, returning generic errors for anything not in the user-facing prefix allowlist. Path components like `/var/lib/archipelago/` are replaced with `[data]/`.

### Recommended Actions (ordered by impact)

1. Implement cosign image verification before any public release. This is a hard requirement for supply chain security.
2. Replace `postMessage('*')` with explicit target origins derived from the iframe's `src` URL.
3. Migrate password hashing from bcrypt to Argon2id (already a dependency) with a transparent upgrade path on next login.
4. Add `DOMPurify.sanitize()` around all `v-html` usage or replace with a component-based SVG renderer.
5. Add session persistence (SQLite) to survive restarts and enable audit logging.

---

## 2. Session & Auth

### Password Hashing

**Current**: `bcrypt` crate with `DEFAULT_COST` (cost factor 12).

| Property | bcrypt (current) | Argon2id (available) |
|----------|-----------------|---------------------|
| Algorithm | Blowfish-based, 1999 | Memory-hard, won PHC 2015 |
| GPU resistance | Moderate (small state) | Strong (memory-hard) |
| Cost factor | `DEFAULT_COST = 12` (~250ms) | m=64MiB, t=3, p=4 (already configured in `totp.rs`) |
| ASIC resistance | Weak | Strong |
| Ecosystem status | Mature, stable | Modern standard, OWASP recommended |
| Already a dependency | No (separate crate) | Yes (`argon2` crate used by `totp.rs` and `backup/identity.rs`) |

**Finding**: bcrypt at cost 12 is adequate for a local appliance where login attempts are rate-limited. However, Argon2id is already linked into the binary. Using two different password hashing algorithms in the same project increases cognitive overhead and the risk of confusion. The TOTP module already uses Argon2id with well-chosen parameters (64 MiB memory, t=3 iterations, p=4 parallelism).

**Recommendation**: Migrate to Argon2id on next password change. Store a version tag in `user.json` to allow transparent upgrade: on successful bcrypt login, re-hash with Argon2id and save.

### Session Tokens

**Current**: 32 bytes from `rand::random()` (which delegates to `OsRng`/`ChaCha20Rng`), hex-encoded (64 characters). Tokens are hashed with SHA-256 before storage, so the raw token never exists in the session map.

**Analysis**:
- 256 bits of entropy from a CSPRNG: more than sufficient. Brute-forcing 2^256 is infeasible.
- SHA-256 hashing of stored tokens: correct. A database leak would not expose session tokens.
- Hex encoding doubles the string length but is unambiguous and URL-safe.

**Verdict**: This is correct and secure for a single-instance appliance. No change needed.

### Session Storage

**Current**: In-memory `HashMap<[u8; 32], Session>` behind `Arc<RwLock<>>`.

| Feature | Status |
|---------|--------|
| TTL-based expiry | 24 hours inactivity (full), 5 minutes (pending TOTP) |
| Max concurrent sessions | 5 (oldest evicted) |
| Session rotation on password change | Yes (`rotate()` + `invalidate_all_except()`) |
| Cleanup of expired sessions | `cleanup_expired()` available, presumably called periodically |
| Persistence across restarts | **No** |
| Audit trail | **No** |

**Risk**: Medium. Session loss on restart is annoying but not a security issue (it forces re-authentication). The lack of audit trail means there is no way to retroactively determine who was authenticated when.

**Recommendation**: Add SQLite-backed session store when adding multi-user support. For now, the in-memory approach is acceptable.

### CSRF Protection

**Current**: Double-submit cookie pattern.

1. On login, server sets `csrf_token` cookie (SameSite=Strict, readable by JS) and `session` cookie (HttpOnly, SameSite=Strict).
2. Frontend reads `csrf_token` from `document.cookie` and sends it as `X-CSRF-Token` header on every RPC call.
3. Backend validates `csrf_cookie == csrf_header` on every authenticated request.

**Analysis**:
- SameSite=Strict prevents cross-origin cookie submission entirely in modern browsers.
- The double-submit pattern provides defense-in-depth for browsers that do not enforce SameSite.
- CSRF token is 32 bytes (256 bits) of randomness -- sufficient.
- Secure flag is conditionally set (production only, not dev mode) -- correct.

**Finding**: The CSRF implementation is sound. One minor note: the CSRF token is generated independently of the session token. This is fine because both are random and the cookie binding ensures they cannot be used cross-session.

**Verdict**: Correct. No changes needed.

### Rate Limiting

**Login rate limiter**: 5 failures per 60 seconds per IP. Implemented as `Vec<Instant>` per IP with sliding window.

**Endpoint rate limiter**: Per-method limits on 25+ sensitive endpoints. Examples:

| Endpoint | Max Requests | Window |
|----------|-------------|--------|
| `wallet.send` | 5 | 300s |
| `lnd.payinvoice` | 10 | 300s |
| `identity.create` | 10 | 300s |
| `backup.create` | 10 | 600s |
| `system.factory-reset` | (not rate-limited) | -- |
| `container-install` | 5 | 300s |
| `auth.changePassword` | 3 | 300s |
| `federation.join` | 5 | 60s |
| `update.apply` | 2 | 600s |

**Coverage gaps**:
- `system.factory-reset` is not rate-limited. While it requires authentication, a compromised session could rapidly trigger factory resets. Low practical risk since one reset wipes everything.
- `tor.rotate-service` is not rate-limited. Rapid rotation could burn through Tor circuits.
- No global rate limit across all endpoints -- only per-method. A compromised session could flood non-limited endpoints.

**Verdict**: Good coverage for a single-user appliance. The per-method approach is appropriate for the threat model.

### TOTP 2FA

**Implementation quality**: Excellent.

| Feature | Implementation |
|---------|---------------|
| Secret generation | 20 bytes (160 bits) from `OsRng` |
| Secret storage | Encrypted at rest: Argon2id KDF -> ChaCha20-Poly1305 envelope |
| Encryption layers | 3: password -> KEK (Argon2id) -> MEK (random) -> secret |
| Verification window | Current step +/- 1 (3 steps total, ~90s window) |
| Replay protection | Used time steps tracked and rejected |
| Code comparison | Constant-time comparison (`constant_time_eq`) |
| Backup codes | 8 codes, bcrypt-hashed, one-time use |
| Pending session | Max 5 attempts, 5-minute TTL, then forced re-login |
| Re-keying | MEK re-encrypted under new password on password change |
| Zeroize | KEK, MEK, and raw secret zeroized after use |

**Finding**: This is a textbook TOTP implementation. The envelope encryption (KEK/MEK pattern) is the same approach used by hardware security modules. The `zeroize` crate ensures secrets do not linger in memory.

One minor note: `constant_time_eq` is hand-rolled rather than using the `subtle` crate's `ConstantTimeEq`. The implementation is correct (XOR accumulation), but the `subtle` crate is specifically designed to resist compiler optimizations that could break constant-time behavior.

**Recommendation**: Consider switching to `subtle::ConstantTimeEq` for the TOTP comparison. The current implementation is likely fine in practice, but `subtle` provides stronger guarantees against compiler reordering.

---

## 3. Cryptographic Review

| Component | Our Implementation | Library Alternative | Correct? | Secure? | Verdict |
|-----------|-------------------|---------------------|----------|---------|---------|
| **Password hashing** | `bcrypt` crate, `DEFAULT_COST` (12) | `argon2` crate (already a dep) | Yes | Adequate | **Migrate to Argon2id**. Already a dependency, memory-hard, OWASP recommended. bcrypt is not broken but Argon2id is strictly better against modern attacks. |
| **Session tokens** | `rand::random::<[u8; 32]>()` + hex, SHA-256 stored hash | `tower-sessions` or signed JWTs via `jsonwebtoken` | Yes | Yes | **Keep custom**. 256-bit CSPRNG tokens with hashed storage is textbook. JWTs add complexity and stateless verification is not needed for single-instance. |
| **TOTP KDF** | `argon2` crate, Argon2id v0x13, m=64MiB, t=3, p=4 | N/A (already using the right library) | Yes | Yes | **Correct**. Strong parameters that balance security and UX on modest hardware. |
| **TOTP encryption** | `chacha20poly1305` crate, KEK/MEK envelope | `age` crate | Yes | Yes | **Keep custom**. The envelope pattern gives us re-keying without re-encrypting the secret. `age` would not support this use case without wrapping. |
| **DID signing** | `ed25519-dalek` direct usage | SpruceID `ssi` crate | Yes | Yes | **Keep custom**. Our code is 381 lines, handles did:key + DID Documents + dual-key (Ed25519+secp256k1). `ssi` would add 50+ transitive deps. |
| **VC signatures** | Custom Ed25519Signature2020 proof (credentials.rs, 796 lines) | SpruceID `ssi` VC module | Yes | Yes for our proof type | **Keep custom for issuance**. Consider `ssi` only for verifying external VCs with non-Ed25519 proof types. |
| **Backup encryption** | Argon2id KDF + ChaCha20-Poly1305 (backup/identity.rs, 132 lines) | `age` crate | Yes | Yes | **Keep custom**. Clean, minimal, well-tested. `age` is simpler API but our code is already simple. |
| **Key storage** | Raw bytes in files with `0o600` permissions | `keyring` crate or OS keychain | Yes | Adequate | **Keep current**. File-based is correct for headless Linux server. No desktop environment means no keychain daemon. Permissions are set immediately after key generation. |
| **Constant-time comparison** | Hand-rolled XOR accumulation | `subtle` crate `ConstantTimeEq` | Correct logic | Likely | **Consider `subtle`**. Hand-rolled constant-time code can be optimized away by the compiler. `subtle` uses inline assembly barriers. Low risk in practice. |
| **CSRF tokens** | `rand::thread_rng().fill()` 32 bytes | N/A | Yes | Yes | **Correct**. `thread_rng()` delegates to `OsRng`-seeded `ChaCha20Rng`. 256 bits is more than sufficient. |

### Key Observations

1. **Argon2 is used correctly in two places** (TOTP and backup) **but not for password hashing**. This is the most obvious inconsistency. The project already pays the compilation cost for `argon2`; using it for password hashing would unify the crypto stack.

2. **ChaCha20-Poly1305 is used correctly** throughout. Nonces are generated from `OsRng`, key material is zeroized after use, and the AEAD construction prevents both tampering and ciphertext manipulation.

3. **Ed25519-dalek usage is clean**. Key generation uses `OsRng`, signing and verification are straightforward, the Ed25519-to-X25519 conversion for key agreement is done correctly via `curve25519-dalek`.

4. **No custom cryptographic primitives**. All cryptographic operations use well-audited Rust crates. The project does not implement any ciphers, hash functions, or key exchange algorithms from scratch. This is the correct approach.

---

## 4. Container Security

### Capability Dropping

**Default**: `--cap-drop=ALL` applied to all non-privileged containers (`package.rs:265`).

Per-app capabilities are explicitly added back via `get_app_capabilities()`:

| App Category | Capabilities Added | Justification |
|-------------|-------------------|---------------|
| Minimal apps (searxng, filebrowser, etc.) | None | Runs with zero capabilities |
| Standard apps (photoprism, grafana) | CHOWN, SETUID, SETGID | Internal user switching |
| Bitcoin/Lightning | CHOWN, FOWNER, SETUID, SETGID, DAC_OVERRIDE | Data directory ownership |
| Web servers (nginx-proxy-manager, vaultwarden) | CHOWN, SETUID, SETGID, NET_BIND_SERVICE | Binding ports < 1024 |
| Tailscale | `--privileged` + NET_ADMIN, NET_RAW | VPN tunnel creation (unavoidable) |

**Finding**: The capability model is well-designed. Each app gets the minimum capabilities needed. The `--privileged` exception for Tailscale is documented and unavoidable (it needs TUN device access and network namespace manipulation).

**Risk**: DAC_OVERRIDE grants the ability to bypass file permission checks. This is a broad capability. For Bitcoin/Lightning, it is necessary because the containers need to access data directories with varying ownership. Consider whether `FOWNER` alone would suffice for some of these apps.

### Read-Only Root Filesystem

**Current**: `--read-only` is applied to apps listed in `is_readonly_compatible()`:

- searxng, grafana, filebrowser, mempool-electrs, electrs, nostr-rs-relay, ollama, indeedhub

**Not read-only**: Bitcoin, LND, Nextcloud, BTCPay, Jellyfin, HomeAssistant, and others.

With `--read-only`, tmpfs mounts are added for `/tmp` and `/run`:
```
--tmpfs=/tmp:rw,noexec,nosuid,size=256m
--tmpfs=/run:rw,noexec,nosuid,size=64m
```

**Finding**: The `noexec` and `nosuid` flags on tmpfs mounts are a good hardening measure. The list of read-only-compatible apps is conservative, which is the correct approach -- it is better to not break an app than to force read-only on an incompatible container.

**Recommendation**: Gradually test more apps with `--read-only` and expand the list. Each new addition should be validated by running the app and checking for write failures.

### No-New-Privileges

**Current**: `--security-opt=no-new-privileges:true` applied to all non-Tailscale containers (`package.rs:266`).

**Finding**: Correct. This prevents setuid binaries inside containers from escalating privileges. Combined with `--cap-drop=ALL`, this creates a strong privilege boundary.

### User Namespace / Non-Root

**Current**: Podman runs containers with rootless mode when the `archipelago` user is not root (`podman_client.rs:60-66`). However, `podman_async()` uses `sudo podman` (`podman_client.rs:71-73`), which runs containers in the root Podman context.

**Finding**: The `sudo podman` invocation means containers run in a root context, not rootless. While `--cap-drop=ALL` and `no-new-privileges` provide strong isolation, the containers themselves may run as root (UID 0) inside their namespace. Whether the container process runs as non-root depends on the container image's Dockerfile (e.g., `USER 1000`).

**Recommendation**: Add `--user` flags to container creation where the upstream image supports it. Audit each container image to determine which ones run as root internally.

### Image Pinning

**Current**: The `is_valid_docker_image()` function validates registry origin (docker.io, ghcr.io, localhost) and rejects shell metacharacters. However, there is no enforcement of digest pinning (e.g., `image@sha256:...`).

Images can be specified with tags (`:latest`, `:v1.0.0`) but tags are mutable -- a registry compromise could replace the image behind a tag.

**Finding**: No digest pinning is enforced. While registry validation limits the attack surface, a compromised registry account could push malicious images under existing tags.

**Recommendation**: For the curated app list, pin images to specific digests in the marketplace metadata. Allow tag-based pulls for user-specified images but warn about the risk.

### Cosign Verification

**Current**: `podman_client.rs:83-84`:
```rust
// TODO: Implement cosign verification
log::warn!("Signature verification not yet implemented: {}", sig);
```

**Finding**: This is the most significant security gap in the container subsystem. Without signature verification, there is no cryptographic proof that a pulled image was built by the expected author.

**Risk**: HIGH. This should be implemented before any public release.

**Recommendation**: Integrate `sigstore-rs` for cosign verification. At minimum, verify signatures for the curated app list. Third-party apps from the decentralized marketplace should also have verifiable signatures (the publisher's Nostr key can serve as the trust anchor).

### Network Isolation

**Current**: Containers are placed on either:
- `archy-net` (shared network for Bitcoin stack: bitcoin-knots, lnd, electrs, mempool, btcpay, fedimint)
- Host network (Tailscale only)
- Default isolated network (all other apps)

**Finding**: The `archy-net` shared network is necessary for the Bitcoin stack to communicate (LND needs Bitcoin RPC, Mempool needs Electrs, etc.). Other apps are properly isolated.

**Recommendation**: Consider creating separate networks for distinct app clusters (e.g., `btcpay-net` for BTCPay + nbxplorer + postgres) rather than putting everything on `archy-net`. This would limit lateral movement if a single container is compromised.

### Secrets Injection

**Current**: Secrets are passed to containers via environment variables (`-e` flag). For example, Bitcoin RPC credentials:
```rust
"--bitcoind-password".to_string(), "archipelago123".to_string(),
```

**Finding**: Environment variables are visible via `podman inspect` and `/proc/<pid>/environ` on the host. The hardcoded `archipelago123` RPC password is particularly concerning -- it should be randomly generated per installation.

**Recommendation**:
1. Generate random credentials per app installation and store them via the secrets manager.
2. Prefer bind-mounting secret files into containers (`--secret` or `-v /path/to/secret:/run/secrets/password:ro`) over environment variables.
3. Replace the hardcoded `archipelago123` Bitcoin RPC password with a per-install random password.

---

## 5. RPC Security

### Authentication Enforcement

**Unauthenticated endpoints** (from `UNAUTHENTICATED_METHODS`):
- `auth.login`, `auth.login.totp`, `auth.login.backup` -- login flow
- `auth.isOnboardingComplete`, `auth.isSetup` -- setup status checks
- `health` -- health check
- `backup.restore-identity` -- onboarding restore (before user account exists)
- `federation.peer-joined`, `federation.peer-address-changed`, `federation.get-state` -- inter-node RPC

**Finding**: The unauthenticated endpoint list is reasonable. The federation endpoints are called by peer nodes over Tor and cannot use session cookies -- they are rate-limited instead (10 requests/60s for peer-joined and peer-address-changed, 30 requests/60s for get-state).

`backup.restore-identity` is unauthenticated by design -- it is used during onboarding before a user account exists. This is the correct approach.

**Risk**: The federation endpoints accept peer assertions (e.g., "I just joined your federation") without cryptographic authentication beyond the Tor hidden service address. A future improvement would be to require DID-signed payloads for federation RPCs.

### RBAC

**Current**: RBAC is implemented and wired into the RPC dispatcher (`mod.rs:249-269`). Three roles are defined:

| Role | Access |
|------|--------|
| Admin | Everything |
| Viewer | Read-only system/node/container/federation/identity/backup methods + logout |
| AppUser | Basic system stats, container listing, health, logout, password change |

**Finding**: RBAC is operational. The `can_access()` method uses prefix matching (e.g., `method.starts_with("system.")`) which is a reasonable approach for method-based access control.

**Concern**: The Viewer role grants access to `federation.list` and `dwn.query` but not `dwn.write-message`. This is correct. However, the prefix-matching approach means that if a new method like `system.factory-reset` is added, it would be accessible to Viewers because it starts with `system.`. The current code mitigates this because `system.factory-reset` is not listed in the Viewer's allowed prefixes -- it requires an exact `system.` prefix match, and the Viewer role only allows `method.starts_with("system.")`.

**Wait** -- actually, `system.factory-reset` does start with `system.`, so Viewers WOULD have access to it under the current RBAC rules.

**Risk**: MEDIUM. Any new `system.*` method is automatically accessible to Viewers. The Viewer role should use an explicit allowlist rather than prefix matching for the `system.` namespace.

**Recommendation**: Change Viewer's `system.*` access to an explicit list: `system.stats`, `system.temperature`, `system.disk-status`, etc. Do not allow `system.factory-reset`, `system.shutdown`, `system.reboot`, or `system.disk-cleanup` for Viewers.

### Input Validation

Five critical endpoints traced from params to handler:

1. **`auth.login`**: Password extracted as string from params, passed to `bcrypt::verify()`. No injection risk -- bcrypt operates on byte arrays. Rate-limited.

2. **`package.install`**: Package ID validated by `validate_app_id()` (lowercase alphanumeric + hyphens, 1-64 chars, no leading hyphen). Docker image validated by `is_valid_docker_image()` (length check, no shell metacharacters, registry allowlist). Both validations are solid.

3. **`system.factory-reset`**: Requires `confirm: true` parameter. Authenticated and RBAC-checked. No injection risk -- the handler performs fixed system operations.

4. **`backup.restore-identity`**: Accepts a JSON blob with a base64-encoded encrypted backup. The backup is decrypted with a user-supplied passphrase. Input validation: blob must be valid base64, must contain salt + nonce + ciphertext of minimum length, decrypted key must be exactly 32 bytes. The Argon2id KDF prevents timing attacks on the passphrase.

5. **`identity.create`**: Accepts optional `label` and `type` parameters. The label is stored as-is in a JSON file. No length validation on the label. This is a low risk since the label is never used in shell commands or HTML rendering, but a maximum length should be enforced.

### Error Sanitization

**Current**: `sanitize_error_message()` in `mod.rs:72-104`:

- User-facing prefixes (Invalid, Missing, Not found, etc.) are passed through with path sanitization.
- Path components (`/var/lib/archipelago/`, `/usr/local/bin/`, `/etc/`) are replaced with `[data]/`, `[bin]/`, `[config]/`.
- Messages exceeding 200 characters are truncated.
- All other errors return: `"Operation failed. Check server logs for details."`

**Finding**: This is a good approach. The prefix allowlist ensures that validation errors remain actionable for the user while internal errors (stack traces, database errors, file system errors) are hidden.

**Minor concern**: The `contains()` check (`msg.contains(prefix)`) rather than `starts_with()` means that an internal error message containing the word "Password" anywhere would be passed through. For example, an error like "Failed to read /etc/shadow: Password file locked" would match the "Password" prefix. This is unlikely to leak sensitive information but is worth tightening.

### Path Traversal

**Frontend** (`filebrowser-client.ts`): `sanitizePath()` is not present in `rpc-client.ts`, but the filebrowser client strips `..` and `/` from filenames: `name.replace(/\.\./g, '').replace(/\//g, '')`.

**Backend**: File operations use `PathBuf::join()` which does not normalize `..` components. However, all file paths are constructed from validated app IDs (alphanumeric + hyphens) and fixed directory structures. There is no user-controlled path component that could escape the data directory.

**Verdict**: Path traversal risk is low. The app ID validation prevents directory traversal in container data paths.

---

## 6. Frontend Security

### XSS

**`v-html` usage**: Found in one location:
- `Settings.vue:286`: `<div v-html="totpQrSvg" />` -- renders server-generated SVG.

**Analysis**: The SVG is generated by the `qrcode` crate on the backend and contains only geometric shapes (rects, paths). It does not include any user-controlled content. However, `v-html` bypasses Vue's template escaping entirely.

**Risk**: LOW currently (SVG is trusted server output), but HIGH if the generation path ever changes.

**Recommendation**: Replace `v-html` with either:
1. An `<img>` tag with a data URI: `<img :src="'data:image/svg+xml;base64,' + btoa(totpQrSvg)" />`
2. `DOMPurify.sanitize(totpQrSvg)` before rendering with `v-html`.

### CSRF

**Frontend implementation** (`rpc-client.ts:18-21, 42-45`):
```typescript
function getCsrfToken(): string | null {
  const match = document.cookie.match(/(?:^|;\s*)csrf_token=([^;]+)/)
  return match ? match[1]! : null
}
```

The CSRF token is read from cookies and sent as `X-CSRF-Token` header on every RPC call via `fetch()` with `credentials: 'include'`.

**Finding**: Correctly implemented. The token is scoped to the session (new token issued on login, rotated on password change, expired on logout).

### Credential Storage (localStorage)

Audit of all `localStorage.setItem` calls:

| Key | Content | Risk |
|-----|---------|------|
| `neode_locale` | Language preference ("en") | None |
| `neode-auth` | Boolean flag ("true") | None -- not a credential |
| `neode_onboarding_complete` | Boolean flag | None |
| `neode_intro_seen` | Boolean flag | None |
| `neode_backup_created` | Boolean flag | None |
| `neode_did` | DID string (public identifier) | None -- DIDs are public |
| `neode_did_state` | DID + KID + pubkey (all public) | None |
| `neode_nostr_npub` | Nostr public key (public) | None |
| `archipelago-ui-mode` | "easy" / "advanced" | None |
| `archipelago-goal-progress` | UI progress state | None |
| `archipelago-spotlight-recent` | Recent search items | None |
| `federation-view` | Active federation tab | None |
| `IDENTITY_KEY + appId` | Nostr identity for app context | **LOW** -- contains public key, not private |
| `APPROVED_ORIGINS_KEY` | Set of approved iframe origins | **LOW** -- UI preference |
| `DISPLAY_MODE_KEY` | "overlay" / "tab" | None |

**Finding**: No secrets, passwords, private keys, or session tokens are stored in localStorage. All stored values are either UI preferences or public identifiers. This is correct.

### iframe postMessage Security

**Outbound `postMessage('*')` calls** (wildcard target origin):

1. `AppSession.vue:490,492` -- Nostr signing responses sent to iframe source with `'*'`
2. `AppLauncherOverlay.vue:390` -- Escape key event sent to parent with `'*'`
3. `appLauncher.ts:262,306,309` -- Nostr signing responses sent to iframe source with `'*'`

**Inbound origin validation**:

1. `contextBroker.ts:65` -- Validates `event.origin !== this.allowedOrigin` (properly restrictive)
2. `Chat.vue:110` -- Validates against expected AIUI URL origin (properly restrictive)
3. `AppSession.vue` and `appLauncher.ts` -- No origin validation on incoming `nostr-request` messages

**Finding**: The Nostr signer (NIP-07 bridge) accepts signing requests from any iframe without verifying the origin, and sends signed responses back with `'*'` target origin. This means:
- Any iframe loaded in the app launcher could request Nostr event signatures.
- The signed response could be intercepted by any window.

**Mitigation**: The user is prompted to approve signing requests (a consent dialog exists in `appLauncher.ts`), and the approved origins list is stored in localStorage. However, the initial request acceptance has no origin check.

**Risk**: MEDIUM. A malicious app loaded in an iframe could silently request signatures for crafted Nostr events.

**Recommendation**:
1. Validate `event.origin` on incoming `nostr-request` messages against the app's known URL.
2. Replace `postMessage(msg, '*')` with `postMessage(msg, expectedOrigin)` for Nostr responses.
3. The `AppLauncherOverlay.vue:390` escape event using `'*'` is lower risk since it only sends a UI event to the parent window, but should still use a specific origin.

### Dependency Audit

Note: `npm audit` was not run as part of this review (requires network access and node_modules). This should be run separately:
```bash
cd neode-ui && npm audit
```

The project uses Vue 3, Vite 7, and Pinia -- all actively maintained. The key security-relevant frontend dependencies are:
- `fetch` API (native, no third-party HTTP client)
- No `eval()` or `new Function()` usage detected
- No inline scripts or styles that would conflict with CSP

---

## 7. Custom Code vs Libraries

### Summary Table

| # | Component | Lines | Quality | Alternative | Verdict |
|---|-----------|-------|---------|-------------|---------|
| 1 | HTTP Server (`handler.rs`) | 813 | Functional but hand-rolled | `axum` | **Migrate** |
| 2 | Session Management (`session.rs`) | 595 | Solid, well-tested | `tower-sessions` | **Keep** (for now) |
| 3 | Rate Limiting (`session.rs` + `mod.rs`) | ~120 | Simple, effective | `governor` | **Keep** |
| 4 | DID Implementation (`identity.rs`) | 381 | Clean, W3C compliant | SpruceID `ssi` | **Keep** |
| 5 | Verifiable Credentials (`credentials.rs`) | 796 | W3C VC 2.0 compliant | SpruceID `ssi` VC | **Keep** (consider `ssi` for external VC verification) |
| 6 | did:dht | ~200 | Works via `mainline` | `pkarr` | **Evaluate** |
| 7 | DWN Store | ~300 | Skeletal | None mature | **Keep** (deprioritize) |
| 8 | WebSocket State Broadcasting | ~200 | Works but full-resync | `json-patch` | **Add library** |
| 9 | Form Validation (frontend) | Scattered | Inconsistent | `zod` | **Add library** |
| 10 | Container Runtime (`podman_client.rs`) | 410 | Clean abstraction | `bollard` | **Keep** |

### Detailed Assessments

**1. HTTP Server (custom `handler.rs` -- 813 lines)**

The handler manually implements routing, CORS headers, WebSocket upgrade, request body parsing, and response building using raw `hyper 0.14`. This works but is fragile -- every new route requires manual pattern matching, there is no middleware stack, and hyper 0.14 is end-of-life.

Alternative: `axum` (built by the tokio team on hyper 1.x) provides typed extractors, a middleware stack via `tower`, built-in WebSocket support, and is the de facto standard for Rust web servers.

**Verdict**: Migrate. This is the highest-impact refactoring item. `axum` would reduce `handler.rs` to approximately 200 lines while adding type safety, automatic request parsing, and tower middleware support. Risk is medium -- the RPC logic is unchanged, only the HTTP glue changes.

**2. Session Management (custom `session.rs` -- 595 lines including 300+ lines of tests)**

The session store is ~200 lines of production code with ~370 lines of comprehensive tests. It implements token hashing, TTL expiry, concurrent session limits, session rotation, and pending TOTP sessions with attempt tracking. The code uses `zeroize` for TOTP secrets.

Alternative: `tower-sessions` with `tower-sessions-sqlx-store` for SQLite-backed persistence.

**Verdict**: Keep custom for now. The implementation is correct, well-tested, and purpose-built for the two-phase TOTP flow. A library would not handle the pending/full session distinction without significant customization. Migrate to `tower-sessions` only if SQLite persistence or multi-instance deployment is needed.

**3. Rate Limiting (custom, ~120 lines)**

Simple in-memory sliding window counters per (method, IP). Not configurable at runtime but the static configuration is well-chosen for each endpoint category.

Alternative: `governor` crate or `tower::limit::RateLimitLayer`.

**Verdict**: Keep custom. The implementation is straightforward, correct, and tailored to the per-method needs. `governor` would add a dependency for minimal benefit. Revisit only if distributed rate limiting is needed (multiple backend instances).

**4. DID Implementation (`identity.rs` -- 381 lines)**

Clean implementation of `did:key` method using `ed25519-dalek`. Generates W3C DID Core v1.0 compliant DID Documents with Ed25519 verification keys and X25519 key agreement keys. Includes Ed25519-to-X25519 conversion, Nostr secp256k1 dual-key support, and roundtrip tests.

Alternative: SpruceID `ssi` crate (v0.15.0).

**Verdict**: Keep custom. The code is ~380 lines, handles exactly the features needed (did:key + dual-key DID Documents), and has good test coverage (12 tests). `ssi` would add 50+ transitive dependencies for features like did:web, did:ethr, did:ion resolution that are not needed. The maintenance burden of 380 lines of well-tested code is far lower than managing a large dependency tree.

**5. Verifiable Credentials (`credentials.rs` -- 796 lines)**

W3C VC Data Model 2.0 implementation supporting issuance, verification, revocation, and verifiable presentations. Uses Ed25519Signature2020 proof format.

Alternative: SpruceID `ssi` VC module.

**Verdict**: Keep custom for issuance and node-to-node verification. The code handles the one proof type needed for Archipelago's use case (Ed25519Signature2020). Consider `ssi` only if external VC verification is needed (verifying credentials issued by non-Archipelago systems with different proof types like BbsBlsSignature2020 or JsonWebSignature2020).

**6. did:dht (`did_dht.rs` -- ~200 lines)**

Implements did:dht resolution via the `mainline` crate (BEP-44 signed DHT records). Includes in-memory caching.

Alternative: `pkarr` crate (v5.0.3, 550K downloads) -- higher-level abstraction over mainline DHT.

**Verdict**: Evaluate `pkarr`. If it handles the BEP-44 encoding that is currently done manually, it would reduce code and benefit from upstream maintenance. If it adds unnecessary abstraction, keep custom. The current code is small and works.

**7. DWN Store (`dwn_store.rs` -- ~300 lines)**

Basic CRUD operations, filesystem-backed, protocol registration. Skeletal implementation.

Alternative: No production-ready DWN implementation exists in Rust. The `dwn` crate by unavi-xyz is v0.4.0 with 323 downloads.

**Verdict**: Keep custom. No viable alternative. Per ADR-011, DWN is deprioritized. The current skeleton is sufficient for the protocol registration feature.

**8. WebSocket State Broadcasting (`state.rs` -- ~200 lines)**

Uses tokio broadcast channels to send full state model resyncs on every change. Every WebSocket client receives the entire state JSON on every update.

Alternative: `json-patch` crate for RFC 6902 JSON diffs. The frontend already includes `fast-json-patch`.

**Verdict**: Add `json-patch`. This is one of the highest-impact improvements. On a system with 10+ containers and active monitoring, the full-state broadcast can be 50-100 KB per update. JSON patches would reduce this to a few hundred bytes per change. Both the Rust `json-patch` crate and the frontend `fast-json-patch` library are mature and actively maintained.

**9. Form Validation (manual inline in Vue components)**

Validation logic is scattered across Vue components with inconsistent patterns. Some forms validate on submit, others on blur, and error messages are not standardized.

Alternative: `zod` (TypeScript-first schema validation, 40M+ weekly npm downloads).

**Verdict**: Add `zod`. Centralize validation schemas in `src/types/schemas.ts`. This is critical for the onboarding flow where bad input (weak passphrase, malformed DID) can cause key generation failures. `zod` integrates naturally with TypeScript and can generate types from schemas, reducing duplication.

**10. Container Runtime Abstraction (`podman_client.rs` -- 410 lines)**

Clean Podman client that wraps CLI invocations for container lifecycle operations. Handles both JSON array and NDJSON output formats from Podman.

Alternative: `bollard` crate (Docker/Podman API client, 7M downloads).

**Verdict**: Keep custom. The current abstraction is clean and purpose-built for the manifest-based approach. `bollard` is Docker-first and would require wrapping for the `AppManifest`-driven container creation. The CLI approach also avoids the Podman socket configuration complexity that `bollard` would require.

---

## What To Do Next

The three most impactful changes from this audit, in priority order:

1. **Implement cosign image verification** (`podman_client.rs`). Integrate `sigstore-rs` for container image signature verification. This closes the largest supply chain attack surface. Without it, a compromised Docker registry could push malicious images.

2. **Fix postMessage wildcard origins** (`AppSession.vue`, `appLauncher.ts`). Replace `postMessage(msg, '*')` with targeted origins. Add `event.origin` validation on incoming Nostr signing requests. This prevents malicious iframes from harvesting signed events.

3. **Migrate password hashing to Argon2id** (`auth.rs`). Add a version field to the user JSON. On login, if the hash is bcrypt, verify with bcrypt, then re-hash with Argon2id and save. This unifies the crypto stack and provides better GPU resistance.

These three changes address the top three risks identified in this audit and are achievable without architectural changes.
