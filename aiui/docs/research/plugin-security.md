# Plugin System Hardening Research — AIUI

## Current State

The plugin system has these existing components:
- `packages/core/src/types/plugin.ts` — `AIUIPlugin` interface, `PluginContext`, `PluginType`
- `packages/core/src/plugins/registry.ts` — in-memory Vue ref-based registry
- `packages/app/src/stores/pluginMarketplace.ts` — `InstalledPlugin`, `PluginPermission`, `installPlugin`, `hasPermission`
- `packages/app/src/components/settings/PluginMarketplace.vue` — permissions dialog
- `packages/app/src/components/renderers/CodeRunner.vue` — existing `<iframe sandbox="allow-scripts">` + postMessage pattern

**Gaps:** No cryptographic signature verification, no runtime permission enforcement in sandbox, no CSP on plugin iframes.

## 1. Signature Validation for Community Plugins

### Problem

`importFromUrl` fetches arbitrary `aiui-plugin.json` from any URL with no integrity check. A compromised URL grants arbitrary code execution.

### Recommended: Ed25519 Detached Signatures

Aligns with existing crypto posture (tweetnacl.js for E2E, Web Crypto for storage).

**Manifest format:**
```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.2.0",
  "signature": {
    "algorithm": "ed25519",
    "publicKey": "base64-public-key",
    "value": "base64-signature-over-canonical-manifest"
  }
}
```

**Verification:** Canonical payload = manifest JSON minus `signature` field, sorted keys. Verify via `crypto.subtle.verify('Ed25519', ...)` (Chrome 113+, Firefox 130+, Safari 17+) with `tweetnacl.js` fallback.

**Integration point:** Gate `installPlugin()` on signature verification for Tier 2+ plugins.

### Secondary: SRI Hash

`bundleUrl` + `integrity` field enables browser-native subresource integrity enforcement at load time.

### Trust Model

| Tier | Who | Signing | Sandbox | Verification |
|---|---|---|---|---|
| 1 — Built-in | AIUI maintainers | Bundled in app | None (trusted) | None needed |
| 2 — Verified | Reviewed by AIUI team | Ed25519 by author | Full iframe sandbox | Signature + hash |
| 3 — Sideloaded | User-imported URL | Optional | Full iframe sandbox, stricter CSP | Warn prominently |

## 2. Sandboxed iframe Execution

### Architecture

Each community plugin runs in an isolated iframe. AIUI manages a `PluginBridge` service:

```
AIUI App (parent)
    │
    │  postMessage (structured protocol)
    │
    └── Plugin Sandbox iframe (origin: null, sandbox="allow-scripts")
            └── Plugin code (no DOM, no storage, no cross-origin network)
```

### Sandbox Configuration

```html
<!-- Tier 2 plugin (headless, no UI) -->
<iframe sandbox="allow-scripts" srcdoc="..." style="display: none" />

<!-- Tier 2 plugin with UI panel -->
<iframe sandbox="allow-scripts allow-popups-to-escape-sandbox" srcdoc="..." />
```

**Never grant:** `allow-same-origin` (breaks isolation), `allow-forms`, `allow-top-navigation`, `allow-modals` unless explicitly user-approved.

### postMessage Protocol

Typed message envelopes following the archyBridge pattern:

```typescript
// Plugin → Host
interface PluginRequest {
  type: 'plugin:request'
  id: string          // correlation ID
  pluginId: string
  capability: string  // e.g. 'storage:get', 'network:fetch'
  payload: unknown
}

// Host → Plugin
interface PluginResponse {
  type: 'plugin:response'
  id: string
  success: boolean
  data?: unknown
  error?: string
}
```

### Host-side Validation

1. `event.origin` must be `null` (sandboxed srcdoc iframes)
2. `pluginId` must match the iframe→plugin mapping
3. Requested capability must be in `grantedPermissions`
4. Rate-limit: reject if > N requests/second (DoS prevention)

### Network Restriction

With `sandbox="allow-scripts"` alone, iframes can still `fetch()`. Block direct network via CSP in srcdoc:

```html
<meta http-equiv="Content-Security-Policy"
  content="default-src 'none'; script-src 'unsafe-inline'; connect-src 'none'">
```

Plugins requiring network use the `network:fetch` capability — host proxies the request after validating the URL.

### Storage Isolation

Sandboxed iframes without `allow-same-origin` cannot access parent's localStorage/IndexedDB. Plugin storage goes through the `storage` capability, namespaced under `plugin::{id}::`.

## 3. Permission System Per Plugin

### Expanded Permissions

```typescript
export type PluginPermission =
  | 'chat-read'        // read chat history
  | 'chat-inject'      // inject messages (high risk)
  | 'chat-messages'    // read + inject (legacy, maps to both)
  | 'network'          // outbound HTTPS via host proxy
  | 'favorites'        // read/write favorites
  | 'storage'          // namespaced plugin storage
  | 'nostr'            // access Nostr identity (high risk)
  | 'wallet'           // deep-link to wallet (high risk)
  | 'clipboard'        // read/write clipboard
  | 'notifications'    // send notifications
  | 'media-playback'   // control media player
  | 'renderer'         // register content renderer
  | 'settings-read'    // read AIUI settings
```

### Risk Classification

```typescript
const permissionRisk: Record<PluginPermission, 'low' | 'medium' | 'high'> = {
  'network': 'low',
  'storage': 'low',
  'chat-read': 'low',
  'favorites': 'low',
  'notifications': 'medium',
  'clipboard': 'medium',
  'media-playback': 'medium',
  'renderer': 'medium',
  'settings-read': 'medium',
  'chat-messages': 'high',
  'chat-inject': 'high',
  'nostr': 'high',
  'wallet': 'high',
}
```

### User Consent Flow

1. High-risk permissions default to unchecked in consent dialog
2. Show risk badges (low/medium/high) next to each permission
3. Unverified plugins (Tier 3) show prominent warning before permissions dialog
4. Progressive disclosure: summary before detailed checkboxes

### Permission Revocation

1. Terminate plugin iframe immediately (`iframe.remove()`)
2. Host handler rechecks `hasPermission()` on every capability call
3. Emit `plugin:permission-revoked` event so plugin can react gracefully

### Least Privilege

- Plugins declare minimum permissions in manifest
- All capabilities gated on `hasPermission()` — no admin override
- Storage namespaced under `plugin::{id}::`
- Optional `allowedOrigins[]` in manifest restricts network targets
- Audit log: capability invocations logged with plugin ID

## Integration Points

### Files to modify:

**`packages/core/src/types/plugin.ts`** — Add `tier`, `signature`, `sandbox` fields to manifest type.

**`packages/app/src/stores/pluginMarketplace.ts`** — Add `verifyManifestSignature()` in `installPlugin()`. Add `revokePermission()`. Move `grantedPermissions` to encrypted storage.

**`packages/core/src/plugins/registry.ts`** — Evolve into `PluginHost` service that manages sandbox iframe lifecycle and routes postMessage capability requests.

**`packages/app/src/plugins/index.ts`** — Tier 1 plugins use `registerPlugin()` directly. Tier 2+ load through `PluginHost.loadSandboxed(manifest)`.

## Concrete Next Steps

1. Define `PluginSignature` type and `verifyManifestSignature()` using tweetnacl.js
2. Create `PluginSandbox` service to manage iframe lifecycle + postMessage routing
3. Add CSP meta tag to plugin srcdoc template
4. Implement capability handlers (storage, network, chat) with permission checks
5. Update consent dialog with risk classification badges
6. Add `revokePermission()` with live iframe termination
7. Create CLI tool for plugin authors to sign manifests
