# Archipelago Developer Guide

## Project Structure

```
archy/
├── core/                          # Rust backend
│   └── archipelago/
│       ├── src/
│       │   ├── main.rs            # Entry point, module declarations
│       │   ├── api/rpc/           # RPC endpoint handlers
│       │   │   ├── dispatcher.rs  # Route dispatcher (~380 method arms)
│       │   │   ├── auth.rs        # Login, session, TOTP
│       │   │   ├── container.rs   # Container lifecycle
│       │   │   ├── package/       # Package install/lifecycle/stacks
│       │   │   ├── interfaces.rs  # Network interfaces, WiFi, DNS
│       │   │   ├── federation/    # Federation management
│       │   │   ├── marketplace.rs # Community marketplace
│       │   │   └── ...            # Other endpoint groups (mesh/, identity/, lnd/, tor/, system/)
│       │   ├── auth.rs            # Password hashing, sessions
│       │   ├── config.rs          # Configuration loading
│       │   ├── server.rs          # HTTP/WS server (axum)
│       │   ├── container/         # Podman integration
│       │   ├── network/           # Network management
│       │   │   ├── dns.rs         # DNS configuration
│       │   │   ├── router.rs      # UPnP, diagnostics
│       │   │   └── dwn_*.rs       # DWN protocol
│       │   ├── federation/        # Federation protocol
│       │   ├── marketplace.rs     # Marketplace discovery
│       │   ├── identity.rs        # DID key management
│       │   ├── vpn.rs             # VPN (Tailscale/WireGuard)
│       │   ├── mesh/              # Tri-protocol mesh (Meshtastic/MeshCore/Reticulum)
│       │   └── ...
│       ├── Cargo.toml
│       └── tests/                 # Integration tests
├── neode-ui/                      # Vue 3 frontend
│   ├── src/
│   │   ├── api/                   # RPC client, WebSocket, container client
│   │   │   └── rpc-client.ts      # Central RPC client (all backend calls)
│   │   ├── views/                 # Page components
│   │   │   ├── Home.vue           # Dashboard with system stats
│   │   │   ├── Marketplace.vue    # App store (curated + community)
│   │   │   ├── Server.vue         # Network, VPN, DNS management
│   │   │   ├── Federation.vue     # Federation dashboard
│   │   │   ├── Settings.vue       # User settings
│   │   │   ├── Web5.vue           # DID, DWN, Nostr
│   │   │   └── ...
│   │   ├── stores/                # Pinia state management
│   │   ├── components/            # Reusable UI components
│   │   ├── composables/           # Vue composables
│   │   ├── router/                # Vue Router with guards
│   │   ├── types/                 # TypeScript type definitions
│   │   └── style.css              # Global styles + Tailwind utilities
│   ├── vite.config.ts
│   └── package.json
├── scripts/                       # Deployment and utility scripts
│   ├── first-boot-containers.sh   # ISO first-boot setup
│   └── run-tests.sh               # CI test runner
├── image-recipe/                  # ISO build configuration
│   ├── build-debian-iso.sh
│   └── configs/                   # Nginx, systemd configs
├── docs/                          # Documentation
│   ├── architecture.md
│   ├── app-manifest-spec.md
│   ├── marketplace-protocol.md
│   └── multi-node-architecture.md
├── apps/                          # App manifests (YAML)
├── CLAUDE.md                      # Contributor guide (invariants, build/verify)
└── docs/ROADMAP.md                # Project roadmap
```

## Development Setup

### Prerequisites

- Node.js 20+ and npm for frontend development.
- Rust stable for backend development.
- Linux with Podman, systemd, and Nginx for host integration work.
- Debian 13 is the target runtime for release validation.

### Local Frontend Development

```bash
cd neode-ui
npm install
npm start         # Vite dev server on :8100, mock backend on :5959
```

The dev server at `http://localhost:8100` uses a mock backend.

### Deploying Changes

Release and host-integration builds should run on Linux. Build the backend and
frontend on the target, or cross-build and copy the artifacts across:

```bash
cd core && cargo build --release
cd neode-ui && npm ci && npm run build
```

A deploy then:
1. Copies the build output to the node
2. Builds Rust backend on the server (`cargo build --release`)
3. Builds Vue frontend (`npm run build`)
4. Copies artifacts to production paths
5. Restarts the `archipelago` systemd service
6. Runs a health check

### Running Tests

```bash
# Frontend tests
cd neode-ui && npm test

# Backend tests
cd core && cargo test --all-features

# Both
./scripts/run-tests.sh
```

`scripts/run-tests.sh` can run backend tests on a Linux target when
`ARCHIPELAGO_SSH_HOST` and `ARCHIPELAGO_SSH_KEY` are set.

## Adding a New RPC Endpoint

### 1. Create the Handler

Add a handler method in the appropriate file under `core/archipelago/src/api/rpc/`. If no existing file fits, create a new one.

```rust
// core/archipelago/src/api/rpc/mymodule.rs
use super::RpcHandler;
use anyhow::Result;

impl RpcHandler {
    /// mymodule.action — description of what it does.
    pub(super) async fn handle_mymodule_action(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

        // Your logic here
        let result = do_something(name).await?;

        Ok(serde_json::json!({ "ok": true, "result": result }))
    }
}
```

**Key patterns:**
- Handlers are `pub(super)` — visible only to the RPC router
- Accept `Option<serde_json::Value>` for params (omit for parameterless endpoints)
- Return `Result<serde_json::Value>`
- Use `self.config.data_dir` for data persistence
- Use `anyhow::bail!()` for error responses

### 2. Register the Route

Add the module declaration in `core/archipelago/src/api/rpc/mod.rs`, then add
the route arm to the `dispatch()` match in
`core/archipelago/src/api/rpc/dispatcher.rs`:

```rust
// api/rpc/mod.rs, at the top:
mod mymodule;

// api/rpc/dispatcher.rs, in the dispatch() match statement:
"mymodule.action" => self.handle_mymodule_action(params).await,
```

### 3. Add Module (if new)

If your logic warrants a separate module:

```rust
// core/archipelago/src/main.rs
mod mymodule;  // Add to module declarations
```

### 4. Frontend Client

Add a convenience method to `neode-ui/src/api/rpc-client.ts`:

```typescript
async myAction(params: { name: string }): Promise<{ ok: boolean; result: string }> {
  return this.call({
    method: 'mymodule.action',
    params,
  })
}
```

### 5. Deploy and Test

```bash
curl -X POST http://<node-host>/rpc/v1 \
  -H "Content-Type: application/json" \
  -b "archipelago_session=YOUR_SESSION" \
  -d '{"method":"mymodule.action","params":{"name":"test"}}'
```

## Adding a New Vue Page

### 1. Create the Component

```vue
<!-- neode-ui/src/views/MyPage.vue -->
<template>
  <div>
    <h1 class="text-4xl font-bold text-white mb-2">My Page</h1>
    <div class="glass-card p-6">
      <!-- Content here -->
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'

// State and logic
</script>
```

### 2. Add the Route

In `neode-ui/src/router/index.ts`, add inside the dashboard children:

```typescript
{
  path: 'my-page',
  name: 'my-page',
  component: () => import('@/views/MyPage.vue'),
},
```

### 3. Standards

- Always use `<script setup lang="ts">` — never Options API
- Use `glass-card` for containers, `bg-white/5 rounded-lg` for sub-rows
- Create global CSS classes in `src/style.css` instead of inline Tailwind
- Use `rpcClient` from `@/api/rpc-client.ts` for all backend calls
- Handle loading states and errors for all async operations

## Writing Tests

### Frontend (Vitest)

```typescript
// neode-ui/src/api/__tests__/my-test.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest'

describe('MyFeature', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('should do something', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ result: 'ok' }),
    }))

    // Test your logic
    expect(true).toBe(true)
  })
})
```

### Backend (Rust)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_my_function() {
        let dir = tempdir().unwrap();
        let result = my_function(dir.path()).await.unwrap();
        assert_eq!(result, expected);
    }
}
```

## Code Quality Checklist

- [ ] TypeScript strict mode: no `any`, use `unknown` or proper types
- [ ] No `unwrap()` or `expect()` in production Rust code — use `?`
- [ ] No `console.log` — wrap in `if (import.meta.env.DEV)`
- [ ] No empty catch blocks — log or handle errors
- [ ] Functions under 50 lines
- [ ] `cargo clippy` and `cargo fmt` pass
- [ ] `npx vue-tsc --noEmit` passes
- [ ] Security: validate all inputs, no command injection
- [ ] Container security: readonly_root, no_new_privileges, non-root user

## Contributing

1. Create a feature branch: `git checkout -b feature/my-feature`
2. Make changes following the standards above
3. Test locally: `cd neode-ui && npm test`
4. Verify on an Archipelago node
5. Commit with conventional format: `feat: add my feature`
