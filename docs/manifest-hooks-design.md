# Manifest Lifecycle Hooks — Design

**Status:** implemented through Phase 4 (see §6; updated 2026-07-08) — only declarative `pre_start` remains · originally Task #20
(indeedhub, netbird) off legacy Rust installers.

See `docs/APP-PACKAGING-MIGRATION-PLAN.md`
("controlled hooks").

---

## 1. Problem

Some apps need a step the static manifest can't express: a **post-start container
mutation**. The motivating case is indeedhub's `patch_indeedhub_nostr_provider()`:

1. `podman exec indeedhub sed -i '/X-Frame-Options/d' /etc/nginx/conf.d/default.conf`
   (strip the header so the app loads in our iframe)
2. `podman cp /opt/archipelago/web-ui/nostr-provider.js indeedhub:/usr/share/nginx/html/`
3. patch nginx conf to inject `<script src="/nostr-provider.js">` and reload

A manifest `files:` entry writes files on the **host** before create; it cannot
patch a **running** container or copy a host file into it. Without a hook,
migrating indeedhub to the orchestrator ships a broken UI.

## 2. Non-goals / security posture

Per the packaging plan: **NOT arbitrary host scripts.** Hooks are declarative,
allowlisted operations, run against the app's **own** (already manifest-sandboxed)
container. This preserves "no arbitrary privileged execution" while giving a
reviewed escape hatch.

- **No host execution.** `exec` runs *inside the container* (`podman exec`), never
  on the host.
- **No arbitrary host reads.** `copy_from_host.src` is **relative to an allowlist
  root** (`<data_dir>` and `/opt/archipelago/web-ui`), resolved + canonicalised;
  any `..` escape or absolute path outside the allowlist is rejected at validate().
- **Same privileges as the container.** `exec` inherits the container's caps
  (already dropped per `security:`), so a hook can't exceed the app's own sandbox.
- **Best-effort + idempotent.** Hooks must be safe to re-run (guard with
  `grep -q … || …`). A hook failure is logged, not fatal — matching the legacy
  best-effort patch, so a transient hook error never bricks an install.

## 3. Schema (`AppDefinition.hooks`)

```yaml
app:
  id: indeedhub
  hooks:
    post_install:        # after the container is created + running, on install
      - exec: ["sed", "-i", "/X-Frame-Options/d", "/etc/nginx/conf.d/default.conf"]
      - copy_from_host:
          src: "web-ui/nostr-provider.js"        # relative to allowlist root
          dest: "/usr/share/nginx/html/nostr-provider.js"
      - exec: ["sh", "-c", "grep -q nostr-provider /etc/nginx/conf.d/default.conf || sed -i 's#</head>#<script src=\"/nostr-provider.js\"></script></head>#' /etc/nginx/conf.d/default.conf"]
      - exec: ["nginx", "-s", "reload"]
    pre_start: []        # (future) run before each start — repair/ownership
```

Types (in `archipelago-container`):
```rust
pub enum HookStep {
    Exec { exec: Vec<String> },
    CopyFromHost { copy_from_host: HostCopy },
}
pub struct HostCopy { pub src: String, pub dest: String }
pub struct LifecycleHooks {
    #[serde(default)] pub post_install: Vec<HookStep>,
    #[serde(default)] pub pre_start: Vec<HookStep>,
}
```
`hooks` is `#[serde(default)]` + forward-compatible (absent = no hooks).

## 4. Execution

`container::hooks::run_post_install(manifest, container_name, data_dir)`:
- Resolve container name via `compute_container_name`.
- For each step in order:
  - `Exec` → `podman exec <container> <args…>` (timeout-bounded).
  - `CopyFromHost` → canonicalise `src` against the allowlist roots; reject on
    escape; `podman cp <abs-src> <container>:<dest>`.
- Log each step; on error, `warn!` and continue (best-effort).

Called from the orchestrator's install path **after** the container is up
(post-create/health), and gated so it runs on install (not every reconcile).
Validation (`AppManifest::validate`): every `copy_from_host.src` must resolve
inside an allowlist root and contain no `..`; `exec` must be non-empty.

## 5. indeedhub migration (the payoff)

With hooks, indeedhub becomes fully manifest-driven: 7 member manifests
(postgres/redis/minio/relay/api/ffmpeg/frontend) + the frontend manifest carries
the `post_install` hook above. `install_indeedhub_stack` becomes orchestrator-first
(like btcpay), legacy as fallback. Same pattern unblocks netbird's setup steps.

## 6. Phases

1. ✅ **Schema + validation + unit tests** — `LifecycleHooks`/`HookStep`/`HostCopy`
   in `archipelago-container::manifest`, allowlist-enforced at `validate()`.
   (commit `4c1a4e59`)
2. ✅ **Executor + wire into orchestrator install** — `container::hooks::run_post_install`
   (`exec` + `copy_from_host`, canonicalise + symlink-escape prefix check, best-effort);
   called from `install_fresh` after the container is up, fresh-container-only.
   (commit `955c54b7`)
3. ✅ **indeedhub**: member manifests + frontend `post_install` hooks shipped
   (`apps/indeedhub/manifest.yml` declares the nostr-provider copy + nginx
   reload; `install_indeedhub_stack` is orchestrator-first via
   `install_stack_via_orchestrator`).
4. ✅ **netbird** (resolved differently): installs via the stack orchestrator,
   but its setup is handled by `generated_secrets`/`generated_certs` + the
   per-app Rust `run_pre_start_hooks` path rather than manifest hooks — no
   `hooks:` block in its manifest.
5. ⏳ `pre_start` hooks (repair/ownership) — type exists; executor not yet
   wired. Note: `prod_orchestrator.rs::run_pre_start_hooks` is a hardcoded
   per-app Rust match today, NOT this declarative path.
