# Archipelago API Reference

All endpoints use JSON-RPC over HTTP POST to `/rpc/v1`.

**Request format:**
```json
{
  "method": "namespace.action",
  "params": { ... }
}
```

**Response format:**
```json
{
  "result": { ... }
}
```

**Error format:**
```json
{
  "error": { "message": "Error description" }
}
```

**Authentication:** All endpoints require a valid session cookie (`archipelago_session`) except those marked "No Auth".

---

## Authentication

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `auth.login` | `{ password: string }` | `{ ok: bool, totp_required?: bool }` | No Auth |
| `auth.logout` | — | `{ ok: bool }` | Yes |
| `auth.changePassword` | `{ current: string, new: string }` | `{ ok: bool }` | Yes |
| `auth.isOnboardingComplete` | — | `{ complete: bool }` | No Auth |
| `auth.onboardingComplete` | — | `{ ok: bool }` | Yes |
| `auth.resetOnboarding` | — | `{ ok: bool }` | Yes |

### TOTP 2FA

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `auth.totp.setup.begin` | `{ password: string }` | `{ secret: string, qr_uri: string, backup_codes: string[] }` | Yes |
| `auth.totp.setup.confirm` | `{ code: string }` | `{ ok: bool }` | Yes |
| `auth.totp.disable` | `{ password: string }` | `{ ok: bool }` | Yes |
| `auth.totp.status` | — | `{ enabled: bool }` | Yes |
| `auth.login.totp` | `{ code: string }` | `{ ok: bool }` | No Auth |
| `auth.login.backup` | `{ code: string }` | `{ ok: bool }` | No Auth |

---

## Container Orchestration

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `container-install` | `{ image: string, name?: string }` | `{ ok: bool, container_id: string }` | Yes |
| `container-start` | `{ id: string }` | `{ ok: bool }` | Yes |
| `container-stop` | `{ id: string }` | `{ ok: bool }` | Yes |
| `container-remove` | `{ id: string }` | `{ ok: bool }` | Yes |
| `container-list` | — | `{ containers: Container[] }` | Yes |
| `container-status` | `{ id: string }` | `{ status: string, ... }` | Yes |
| `container-logs` | `{ id: string, lines?: number }` | `{ logs: string }` | Yes |
| `container-health` | `{ id: string }` | `{ healthy: bool }` | Yes |

## Package Management

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `package.install` | `{ id: string, dockerImage?: string, url?: string, version?: string }` | `{ ok: bool }` | Yes |
| `package.start` | `{ id: string }` | `{ ok: bool }` | Yes |
| `package.stop` | `{ id: string }` | `{ ok: bool }` | Yes |
| `package.restart` | `{ id: string }` | `{ ok: bool }` | Yes |
| `package.uninstall` | `{ id: string }` | `{ ok: bool }` | Yes |

## Bundled Apps

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `bundled-app-start` | `{ id: string }` | `{ ok: bool }` | Yes |
| `bundled-app-stop` | `{ id: string }` | `{ ok: bool }` | Yes |

---

## Node Identity & P2P

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `node.did` | — | `{ did: string }` | Yes |
| `node.signChallenge` | `{ challenge: string }` | `{ signature: string }` | Yes |
| `node.tor-address` | — | `{ address: string }` | Yes |
| `node.nostr-publish` | — | `{ ok: bool, event_id: string }` | Yes |
| `node.nostr-pubkey` | — | `{ pubkey: string }` | Yes |
| `node-nostr-verify-revoked` | — | `{ revoked: bool, nostr_pubkey: string }` | Yes |
| `node-nostr-discover` | — | `{ nodes: DiscoveredNode[] }` | Yes |
| `node-add-peer` | `{ did: string, address: string }` | `{ ok: bool }` | Yes |
| `node-list-peers` | — | `{ peers: Peer[] }` | Yes |
| `node-remove-peer` | `{ did: string }` | `{ ok: bool }` | Yes |
| `node-send-message` | `{ to: string, message: string }` | `{ ok: bool }` | Yes |
| `node-check-peer` | `{ did: string }` | `{ online: bool }` | Yes |
| `node-messages-received` | — | `{ messages: Message[] }` | Yes |
| `node.createBackup` | `{ password: string }` | `{ path: string }` | Yes |

---

## Identity Management

### Multi-Identity

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `identity.list` | `{}` | `{ identities: Identity[] }` | Yes |
| `identity.create` | `{ label: string }` | `{ identity: Identity }` | Yes |
| `identity.get` | `{ id: string }` | `{ identity: Identity }` | Yes |
| `identity.delete` | `{ id: string }` | `{ ok: bool }` | Yes |
| `identity.set-default` | `{ id: string }` | `{ ok: bool }` | Yes |
| `identity.sign` | `{ id: string, data: string }` | `{ signature: string }` | Yes |
| `identity.verify` | `{ id: string, data: string, signature: string }` | `{ valid: bool }` | Yes |
| `identity.resolve-did` | `{ did: string }` | `{ document: DIDDocument }` | Yes |
| `identity.resolve-remote-did` | `{ did: string }` | `{ document: DIDDocument }` | Yes |
| `identity.verify-did-document` | `{ document: object }` | `{ valid: bool }` | Yes |

### Nostr Keys

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `identity.create-nostr-key` | `{ id: string }` | `{ pubkey: string, npub: string }` | Yes |
| `identity.nostr-sign` | `{ id: string, event: object }` | `{ signed_event: object }` | Yes |

### Bitcoin Names (NIP-05)

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `identity.register-name` | `{ name: string, pubkey: string }` | `{ ok: bool }` | Yes |
| `identity.remove-name` | `{ name: string }` | `{ ok: bool }` | Yes |
| `identity.resolve-name` | `{ name: string }` | `{ pubkey: string }` | Yes |
| `identity.list-names` | `{}` | `{ names: NameEntry[] }` | Yes |
| `identity.link-name` | `{ name: string, identity_id: string }` | `{ ok: bool }` | Yes |

### Verifiable Credentials

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `identity.issue-credential` | `{ subject: string, claims: object }` | `{ credential: VC }` | Yes |
| `identity.verify-credential` | `{ credential: object }` | `{ valid: bool }` | Yes |
| `identity.list-credentials` | `{ id?: string }` | `{ credentials: VC[] }` | Yes |
| `identity.revoke-credential` | `{ credential_id: string }` | `{ ok: bool }` | Yes |
| `identity.create-presentation` | `{ credentials: string[] }` | `{ presentation: VP }` | Yes |
| `identity.verify-presentation` | `{ presentation: object }` | `{ valid: bool }` | Yes |

---

## Bitcoin & Lightning

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `bitcoin.getinfo` | — | `{ blocks: number, connections: number, ... }` | Yes |
| `lnd.getinfo` | — | `{ identity_pubkey: string, num_active_channels: number, ... }` | Yes |
| `lnd.listchannels` | — | `{ channels: Channel[] }` | Yes |
| `lnd.openchannel` | `{ pubkey: string, amount: number }` | `{ funding_txid: string }` | Yes |
| `lnd.closechannel` | `{ channel_point: string }` | `{ closing_txid: string }` | Yes |
| `lnd.newaddress` | — | `{ address: string }` | Yes |
| `lnd.sendcoins` | `{ addr: string, amount?: number, send_all?: bool, target_conf?: number, sat_per_vbyte?: number }` | `{ txid: string }` | Yes |
| `lnd.estimatefee` | `{ addr: string, amount: number, target_conf?: number }` | `{ fee_sat: number, sat_per_vbyte: number }` | Yes |
| `lnd.createinvoice` | `{ amount: number, memo?: string }` | `{ payment_request: string }` | Yes |
| `lnd.payinvoice` | `{ payment_request: string }` | `{ preimage: string }` | Yes |
| `lnd.create-psbt` | `{ outputs: object, ... }` | `{ psbt: string }` | Yes |
| `lnd.finalize-psbt` | `{ psbt: string }` | `{ signed_psbt: string }` | Yes |

---

## Ecash Wallet

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `wallet.ecash-balance` | — | `{ balance: number, mint_url: string }` | Yes |
| `wallet.ecash-mint` | `{ amount: number }` | `{ ok: bool }` | Yes |
| `wallet.ecash-melt` | `{ amount: number, invoice: string }` | `{ ok: bool }` | Yes |
| `wallet.ecash-send` | `{ amount: number }` | `{ token: string }` | Yes |
| `wallet.ecash-receive` | `{ token: string }` | `{ amount: number }` | Yes |
| `wallet.ecash-history` | — | `{ transactions: EcashTx[] }` | Yes |
| `wallet.networking-profits` | — | `{ total_sats: number, ... }` | Yes |

---

## Network

### Interfaces & WiFi

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `network.list-interfaces` | — | `{ interfaces: Interface[] }` | Yes |
| `network.scan-wifi` | — | `{ networks: WifiNetwork[] }` | Yes |
| `network.configure-wifi` | `{ ssid: string, password: string }` | `{ ok: bool }` | Yes |
| `network.configure-ethernet` | `{ interface: string, mode: "dhcp"\|"static", ip?: string, gateway?: string, dns?: string }` | `{ ok: bool }` | Yes |
| `network.diagnostics` | — | `{ wan_ip: string, nat_type: string, upnp_available: bool, tor_connected: bool, wifi_count: number }` | Yes |

### DNS

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `network.dns-status` | — | `{ provider: string, servers: string[], doh_enabled: bool, doh_url: string?, resolv_conf_servers: string[] }` | Yes |
| `network.configure-dns` | `{ provider: "system"\|"cloudflare"\|"google"\|"quad9"\|"mullvad"\|"custom", servers?: string[] }` | `{ ok: bool, provider: string, servers: string[], doh_enabled: bool }` | Yes |

### Network Overlay

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `network.get-visibility` | — | `{ visibility: string }` | Yes |
| `network.set-visibility` | `{ visibility: string }` | `{ ok: bool }` | Yes |
| `network.request-connection` | `{ target_did: string }` | `{ request_id: string }` | Yes |
| `network.list-requests` | — | `{ requests: ConnectionRequest[] }` | Yes |
| `network.accept-request` | `{ request_id: string }` | `{ ok: bool }` | Yes |
| `network.reject-request` | `{ request_id: string }` | `{ ok: bool }` | Yes |

### Router / UPnP

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `router.discover` | — | `{ router: RouterInfo }` | Yes |
| `router.list-forwards` | — | `{ forwards: PortForward[] }` | Yes |
| `router.add-forward` | `{ port: number, protocol: string, description: string }` | `{ ok: bool }` | Yes |
| `router.remove-forward` | `{ port: number, protocol: string }` | `{ ok: bool }` | Yes |
| `router.detect` | `{ ... }` | `{ detected: bool, ... }` | Yes |
| `router.info` | — | `{ ... }` | Yes |
| `router.configure` | `{ ... }` | `{ ok: bool }` | Yes |

---

## Tor

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `tor.list-services` | — | `{ services: TorService[] }` | Yes |
| `tor.create-service` | `{ name: string, port: number }` | `{ onion_address: string }` | Yes |
| `tor.delete-service` | `{ name: string }` | `{ ok: bool }` | Yes |
| `tor.get-onion-address` | `{ name: string }` | `{ address: string }` | Yes |

## Nostr Relays

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `nostr.list-relays` | — | `{ relays: RelayConfig[] }` | Yes |
| `nostr.add-relay` | `{ url: string }` | `{ ok: bool }` | Yes |
| `nostr.remove-relay` | `{ url: string }` | `{ ok: bool }` | Yes |
| `nostr.toggle-relay` | `{ url: string }` | `{ ok: bool }` | Yes |
| `nostr.get-stats` | — | `{ total_relays: number, connected: number, enabled: number }` | Yes |

---

## VPN

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `vpn.status` | — | `{ connected: bool, provider?: string, ip_address?: string, hostname?: string, peers_connected: number }` | Yes |
| `vpn.configure` | `{ provider: "tailscale"\|"wireguard", auth_key?: string, address?: string, dns?: string, peer?: object }` | `{ ok: bool }` | Yes |
| `vpn.disconnect` | — | `{ disconnected: bool }` | Yes |

## Mesh Networking

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `mesh.status` | — | `{ enabled: bool, device: string?, nodes: MeshNode[] }` | Yes |
| `mesh.peers` | — | `{ peers: MeshPeer[], count: number }` | Yes |
| `mesh.broadcast` | — | `{ ok: bool }` | Yes |
| `mesh.configure` | `{ enabled: bool, device?: string }` | `{ ok: bool }` | Yes |

---

## Federation

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `federation.invite` | — | `{ code: string }` | Yes |
| `federation.join` | `{ code: string }` | `{ ok: bool, node: FederatedNode }` | Yes |
| `federation.list-nodes` | — | `{ nodes: FederatedNode[] }` | Yes |
| `federation.remove-node` | `{ did: string }` | `{ ok: bool }` | Yes |
| `federation.set-trust` | `{ did: string, trust: "trusted"\|"observer"\|"untrusted" }` | `{ ok: bool }` | Yes |
| `federation.sync-state` | — | `{ results: SyncResult[] }` | Yes |
| `federation.get-state` | — | `{ state: NodeStateSnapshot }` | Federation peer |
| `federation.peer-joined` | `{ did: string, onion: string, pubkey: string }` | `{ accepted: bool }` | Federation peer |
| `federation.deploy-app` | `{ target_did: string, app_id: string, version?: string }` | `{ ok: bool }` | Yes |

---

## Marketplace

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `marketplace.discover` | — | `{ apps: DiscoveredApp[], relay_count: number }` | Yes |
| `marketplace.publish` | `{ app_id, name, version, description, author, container, category, ... }` | `{ ok: bool, event_id: string }` | Yes |
| `marketplace.get-manifest` | `{ app_id: string }` | `DiscoveredApp \| { error: string }` | Yes |
| `marketplace.list-published` | — | `{ manifests: AppManifest[] }` | Yes |
| `marketplace.verify` | `{ ... manifest fields ... }` | `{ valid: bool, issues: string[], trust_score: number }` | Yes |

---

## DWN (Decentralized Web Node)

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `dwn.status` | — | `{ running: bool, message_count: number, protocol_count: number }` | Yes |
| `dwn.sync` | — | `{ synced: number }` | Yes |
| `dwn.register-protocol` | `{ uri: string, definition: object }` | `{ ok: bool }` | Yes |
| `dwn.list-protocols` | — | `{ protocols: Protocol[] }` | Yes |
| `dwn.remove-protocol` | `{ uri: string }` | `{ ok: bool }` | Yes |
| `dwn.query-messages` | `{ protocol?: string, limit?: number }` | `{ messages: DwnMessage[] }` | Yes |
| `dwn.write-message` | `{ protocol: string, data: object }` | `{ ok: bool, message_id: string }` | Yes |

---

## Content Catalog

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `content.list-mine` | — | `{ items: ContentItem[] }` | Yes |
| `content.add` | `{ title: string, type: string, data: object }` | `{ ok: bool, id: string }` | Yes |
| `content.remove` | `{ id: string }` | `{ ok: bool }` | Yes |
| `content.set-pricing` | `{ id: string, price_sats: number }` | `{ ok: bool }` | Yes |
| `content.set-availability` | `{ id: string, available: bool }` | `{ ok: bool }` | Yes |
| `content.browse-peer` | `{ peer_did: string }` | `{ items: ContentItem[] }` | Yes |

---

## System

### Monitoring

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `system.stats` | — | `{ cpu_percent: number, ram_used: number, ram_total: number, disk_used: number, disk_total: number, uptime_secs: number, load_avg: number[] }` | Yes |
| `system.processes` | — | `{ processes: Process[] }` | Yes |
| `system.temperature` | — | `{ celsius: number? }` | Yes |
| `system.detect-usb-devices` | — | `{ devices: UsbDevice[] }` | Yes |

### Updates

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `update.check` | — | `{ available: bool, version?: string, changelog?: string }` | Yes |
| `update.status` | — | `{ state: string, progress?: number }` | Yes |
| `update.dismiss` | — | `{ ok: bool }` | Yes |
| `update.download` | — | `{ ok: bool }` | Yes |
| `update.apply` | — | `{ ok: bool }` | Yes |
| `update.rollback` | — | `{ ok: bool }` | Yes |
| `update.get-schedule` | — | `{ auto_check: bool, auto_install: bool, schedule: string }` | Yes |
| `update.set-schedule` | `{ auto_check?: bool, auto_install?: bool, schedule?: string }` | `{ ok: bool }` | Yes |

### Backup & Restore

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `backup.create` | `{ password: string, include?: string[] }` | `{ path: string, size: number }` | Yes |
| `backup.list` | — | `{ backups: BackupEntry[] }` | Yes |
| `backup.verify` | `{ path: string, password: string }` | `{ valid: bool }` | Yes |
| `backup.restore` | `{ path: string, password: string }` | `{ ok: bool }` | Yes |
| `backup.delete` | `{ path: string }` | `{ ok: bool }` | Yes |
| `backup.list-drives` | — | `{ drives: UsbDrive[] }` | Yes |
| `backup.to-usb` | `{ drive: string, password: string }` | `{ ok: bool }` | Yes |

### Security

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `security.rotate-secrets` | `{ app_id?: string }` | `{ rotated: string[] }` | Yes |
| `security.list-expiring` | `{ days?: number }` | `{ secrets: ExpiringSecret[] }` | Yes |

---

## Utility

| Method | Params | Returns | Auth |
|--------|--------|---------|------|
| `echo` | `{ message: string }` | `{ message: string }` | No Auth |
| `server.echo` | `{ message: string }` | `{ message: string }` | No Auth |

---

## Example: cURL

```bash
# Login (the password you created on the node's first-boot setup screen —
# there is no default password)
curl -c cookies.txt -X POST http://archipelago.local/rpc/v1 \
  -H "Content-Type: application/json" \
  -d '{"method":"auth.login","params":{"password":"YOUR_NODE_PASSWORD"}}'

# Get system stats (authenticated)
curl -b cookies.txt -X POST http://archipelago.local/rpc/v1 \
  -H "Content-Type: application/json" \
  -d '{"method":"system.stats"}'

# Get DID
curl -b cookies.txt -X POST http://archipelago.local/rpc/v1 \
  -H "Content-Type: application/json" \
  -d '{"method":"node.did"}'
```
