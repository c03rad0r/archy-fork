# App secrets

How an app declares a secret, how Archipelago materialises it, and how it
reaches the container — with the rules a developer must not break.

The whole point: **an app never ships a credential.** It declares the *shape* of
the secrets it needs, and the node generates a fresh, per-install value that
never leaves the node and is never logged. Source of truth:
`core/archipelago/src/container/secrets.rs` and the manifest schema in
`core/container/src/manifest.rs`.

## The two halves

A secret has a producer and a consumer, and they are separate manifest fields:

- **`generated_secrets`** — *produce* a random value into a file.
- **`secret_env`** — *inject* a file's contents into the container as an env var.

An app can use either alone. A generated secret with no consumer is just a file
on the node; a `secret_env` with no matching `generated_secrets` reads a file
that some other component (or the daemon) is expected to have written.

## Declaring a generated secret

```yaml
container:
  generated_secrets:
    - name: btcpay-db-password
      kind: hex16
    - name: fedimint-gateway-hash
      kind: bcrypt
```

`name` is a **bare filename** under the node's secrets directory
(`/var/lib/archipelago/secrets/`). It is validated at manifest-load time — no
`/`, no `..` — so a manifest cannot write outside that directory.

`kind` chooses how the value is produced. Each kind is deterministic in *shape*
(the orchestrator knows exactly which files it will create) but random in value:

| `kind`  | Value                                   | Files written                     | Use for |
|---------|-----------------------------------------|-----------------------------------|---------|
| `hex16` | 16 random bytes, lowercase hex (32 ch)  | `<name>`                          | service passwords, API tokens |
| `hex32` | 32 random bytes, lowercase hex (64 ch)  | `<name>`                          | longer keys/cookies |
| `base64`| 32 random bytes, standard base64 (44 ch)| `<name>`                          | services that base64-decode their key (e.g. netbird relay `authSecret`) |
| `bcrypt`| a random password **and** its bcrypt hash| `<name>` (hash) + `<name>.pw` (plaintext) | server configured with a hash, client needs the plaintext |

`bcrypt` is the only kind that writes two files: `<name>` holds the bcrypt hash a
server is configured with, and `<name>.pw` holds the plaintext for any client
that must authenticate against it. A `secret_env` injects whichever of the two it
references.

## Injecting a secret into the container

```yaml
container:
  secret_env:
    - key: BTCPAY_DB_PASS
      secret_file: btcpay-db-password
```

At apply time the orchestrator reads `/var/lib/archipelago/secrets/<secret_file>`
and makes it available in the container as `<key>`. It does **not** do this by
adding `KEY=value` to the environment — that value would show up in
`podman inspect` output and, on the Quadlet path, as a plaintext `Environment=`
line in a unit file on disk. Instead the resolved pairs are registered as podman
secrets named `archy-env-<app-id>-<key>` and referenced by name, so the value
never lands in the manifest, a unit file, `podman inspect`, or a log line.

**Interpolation taints.** A plain `environment` entry that interpolates a secret
— e.g. BTCPay's `ConnectionString=...Password=${BTCPAY_DB_PASS}` — is treated as
secret-bearing itself and travels the same protected path, rather than being
left in the clear because it was declared under `environment`. So you can build
connection strings from secrets without leaking them.

## How materialisation works

`ensure_generated_secrets()` runs on **every install and reconcile tick**, before
`secret_env` is resolved. It is idempotent and self-healing:

1. **Fast path.** If every target file for a secret already exists, is readable
   by the service user, and is non-empty, it is left untouched. A secret is
   generated **once** and then persists across restarts, updates and reinstalls —
   this is what makes credentials stable (migrations never regenerate a working
   secret out from under a database).
2. **Self-heal.** A target file that exists but is unreadable or empty — e.g.
   left root-owned by a botched earlier write — is removed and recreated, owned
   by the service user. The unlink uses the secrets directory's own write bit, so
   recovery needs no privilege escalation.
3. **Write.** New values are written through an atomic `0600` writer: a temp file
   in the same directory, fsynced, then renamed over the target, so a reader never
   sees a half-written secret and the file is only ever readable by its owner.

Because it runs every tick and no-ops when the secret is healthy, calling it is
always safe; there is no separate "provision secrets" step to forget.

## Rules a developer must not break

- **Never hardcode a credential**, in the manifest or in code, even as a
  fallback. A shared fallback password means everyone holding a copy of the repo
  holds that credential. Declare `generated_secrets` instead.
- **Never log a secret.** `secret_env` values and the files under the secrets
  directory stay out of logs, error messages and status output.
- **One canonical name.** The orchestrator, first-boot script, reconcile path and
  any deploy tooling must all reference a secret by the *same* filename. A
  producer writing `<app>-password` while the consumer reads `<app>-hash` yields a
  service that authenticates against a credential nothing generated.
- **Pick the encoding the service expects.** `hex*` and `base64` decode to
  different bytes; a service that base64-decodes its configured key must be given
  a `base64` secret, or it will run with the wrong key material.

## Related

- [App Manifest Specification](app-manifest-spec.md) — the full manifest schema
- [ADR-009: Manifest-Level Container Security](adr/009-manifest-container-security.md)
- [Entropy Enforcement (KEY-05)](security/KEY-05-ENTROPY-ENFORCEMENT.md) — why secret
  generation draws from an explicitly-named CSPRNG
