#!/usr/bin/env node

/**
 * Archipelago Mock Backend Server
 * Pure Archipelago implementation - NO StartOS dependencies
 * Supports dev modes: setup, onboarding, existing
 */

import express from 'express'
import cors from 'cors'
import cookieParser from 'cookie-parser'
import { WebSocketServer } from 'ws'
import http from 'http'
import { exec } from 'child_process'
import { promisify } from 'util'
import fs from 'fs/promises'
import path from 'path'
import { fileURLToPath } from 'url'
import Docker from 'dockerode'
import { AsyncLocalStorage } from 'node:async_hooks'
import crypto from 'crypto'
import { installDemoUploads } from './demo-uploads.js'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

const execPromise = promisify(exec)

// DEMO mode: public, multi-visitor sandbox. Each visitor gets an isolated,
// ephemeral copy of all mutable state (see per-session store below), real
// container runtimes are never touched, and idle sessions are reaped.
// When DEMO is off, behaviour is identical to the classic single-user dev mock.
const DEMO =
  process.env.DEMO === '1' ||
  process.env.VITE_DEMO === '1' ||
  process.env.VITE_DEV_MODE === 'demo'

// Find container socket: Podman (macOS/Linux) or Docker
import { existsSync, readFileSync } from 'fs'
import * as fsSync from 'fs'

// Report the real app version, suffixed with -demo in the public sandbox so it's
// obviously the demo while still tracking whatever version the UI ships.
let APP_VERSION = '0.1.0'
try {
  const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf-8'))
  if (pkg.version) APP_VERSION = pkg.version
} catch { /* fall back to default */ }
if (DEMO) APP_VERSION += '-demo'

function findContainerSocket() {
  // DOCKER_HOST env var (set by podman machine start)
  if (process.env.DOCKER_HOST) {
    const p = process.env.DOCKER_HOST.replace('unix://', '')
    if (existsSync(p)) return p
  }
  // Podman machine socket (macOS) — check TMPDIR-based path
  if (process.env.TMPDIR) {
    const podmanDir = path.join(path.dirname(process.env.TMPDIR), 'podman')
    const sock = path.join(podmanDir, 'podman-machine-default-api.sock')
    if (existsSync(sock)) return sock
  }
  // Docker socket
  if (existsSync('/var/run/docker.sock')) return '/var/run/docker.sock'
  // Linux podman rootless
  const uid = process.getuid?.() || 1000
  const linuxSock = `/run/user/${uid}/podman/podman.sock`
  if (existsSync(linuxSock)) return linuxSock
  return null
}

// In DEMO mode we never bind to a real container runtime — the public demo must
// be host-independent and unable to touch the host's Docker/Podman.
const containerSocket = DEMO ? null : findContainerSocket()
const docker = containerSocket ? new Docker({ socketPath: containerSocket }) : null
if (DEMO) {
  console.log('[Container] DEMO mode — simulation only (real runtime disabled)')
} else if (containerSocket) {
  console.log(`[Container] Socket: ${containerSocket}`)
} else {
  console.log('[Container] No socket found — simulation mode (no Docker/Podman)')
}

const app = express()
// MOCK_BACKEND_PORT lets a harness (scripts/mock-rpc-parity.mjs) bind an
// ephemeral port instead of colliding with a running dev preview on 5959.
const PORT = Number(process.env.MOCK_BACKEND_PORT) || 5959

// Dev mode from environment (setup, onboarding, existing, boot, or default)
const DEV_MODE = process.env.VITE_DEV_MODE || 'default'

// The demo's mesh peer list, shared by mesh.peers and mesh.contacts-list so
// the two can never disagree about who is on the mesh (the daemon merges
// contacts over the same peer map for exactly this reason).
const DEMO_MESH_PEERS = [
  {
    contact_id: 1,
    advert_name: 'archy-198',
    did: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9',
    pubkey_hex: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
    rssi: -67,
    snr: 9.5,
    hops: 0,
  },
  {
    contact_id: 2,
    advert_name: 'satoshi-relay',
    did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH',
    pubkey_hex: 'f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5',
    rssi: -82,
    snr: 4.2,
    hops: 1,
  },
  // No pubkey_hex — mesh.contacts-list omits this peer, matching the daemon's
  // `if let Some(pk) = peer.pubkey_hex` guard.
  {
    contact_id: 3,
    advert_name: 'mountain-node',
    did: null,
    pubkey_hex: null,
    rssi: -95,
    snr: 1.8,
    hops: 2,
  },
  {
    contact_id: 4,
    advert_name: 'bunker-alpha',
    did: 'did:key:z6MkrHKPxJP6tvCvXMaJKZd3rRA2Y44tyftVhR8FDCMKGFjb',
    pubkey_hex: 'c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4',
    rssi: -74,
    snr: 7.1,
    hops: 0,
  },
]

// Ages the peer list's last_heard relative to now, as mesh.peers has always
// reported it (the seed values are offsets in ms).
const DEMO_PEER_LAST_HEARD_MS = { 1: 30000, 2: 120000, 3: 600000, 4: 45000 }

const DEMO_PEER_NAMES = Object.fromEntries(
  DEMO_MESH_PEERS.map((p) => [p.contact_id, p.advert_name]),
)
// This node's own key on the demo. Reactions/replies the visitor sends are
// direction:'sent', which is what the UI keys '__self__' off — but the field
// still has to be present and stable for the message key to work.
const DEMO_SELF_PUBKEY = 'demo02abababababababababababababababababababababababababababab'

// Append a message to this session's mesh store, matching the field set of
// mesh/types.rs MeshMessage (id, direction, peer_contact_id, peer_name,
// plaintext, timestamp, delivered, encrypted, transport, message_type,
// typed_payload, sender_pubkey, sender_seq).
function pushMeshMessage({
  contact_id = null,
  plaintext = '',
  message_type = 'text',
  typed_payload = null,
  transport = 'meshcore',
  channel = null,
}) {
  const meshStore = currentStore().mesh
  const id = 100 + meshStore.dynamic.length
  const msg = {
    id,
    direction: 'sent',
    peer_contact_id: contact_id,
    peer_name: contact_id === null ? (channel === null ? null : `channel-${channel}`) : DEMO_PEER_NAMES[contact_id] || `peer-${contact_id}`,
    plaintext,
    timestamp: new Date().toISOString(),
    delivered: true,
    // mountain-node (3) has no pubkey, so traffic to it is unencrypted — the
    // same asymmetry the rest of the demo already models.
    encrypted: contact_id !== 3,
    transport,
    sender_pubkey: DEMO_SELF_PUBKEY,
    sender_seq: id,
    message_type,
    typed_payload,
  }
  if (channel !== null) msg.channel = channel
  meshStore.dynamic.push(msg)
  return msg
}

// Apply this session's edit/delete overrides over a message list. Keyed by
// sender_seq, mirroring mesh/mod.rs apply_local_edit / apply_local_delete,
// which match an own-Sent message by sender_seq. Kept as an overlay rather
// than mutating the seed array, because the seed is rebuilt on every read.
function applyMeshOverrides(messages) {
  const overrides = currentStore().mesh.overrides
  if (!overrides || Object.keys(overrides).length === 0) return messages
  return messages.map((m) => {
    const o = m.direction === 'sent' && m.sender_seq != null ? overrides[m.sender_seq] : null
    return o ? { ...m, ...o } : m
  })
}

// Boot mode: simulate server startup delay
let BOOT_START_TIME = Date.now()
const BOOT_DELAY_MS = 25000 // 25 seconds of simulated startup (slower for analysis)

// CORS configuration
const corsOptions = {
  credentials: true,
  origin: (origin, callback) => {
    if (!origin) return callback(null, true)
    callback(null, true)
  }
}

app.use(cors(corsOptions))
// Skip JSON body parsing for filebrowser upload routes (binary file bodies)
app.use((req, res, next) => {
  if ((req.path.startsWith('/app/filebrowser/api/resources') && req.method === 'POST') ||
      (req.path.startsWith('/app/filebrowser/api/tus') && req.method === 'PATCH')) {
    return next()
  }
  express.json({ limit: '50mb' })(req, res, next)
})
app.use(cookieParser())

// DEMO: bind every request to an isolated per-visitor state store (keyed by the
// `demo_sid` cookie) for the remainder of the request. Outside DEMO this is a
// no-op and all handlers share the single default store (classic mock behaviour).
app.use((req, res, next) => {
  if (!DEMO) return next()
  const store = resolveSessionStore(req, res)
  stateContext.run(store, () => next())
})

// Mock session storage
const sessions = new Map()
// Public demo uses a memorable shared password (shown on the login screen);
// the classic dev mock keeps password123.
const MOCK_PASSWORD = DEMO ? 'entertoexit' : 'password123'

// Mutable wallet state — faucet/send/receive modify these values.
// SEED_* objects are pristine templates; each demo session gets a deep clone
// (see makeSessionStore). `walletState` itself becomes a session-aware proxy below.
const SEED_WALLET = {
  onchain_sats: 2_350_000,
  channel_sats: 8_250_000,
  ecash_sats: 250_000,
  ecash_tokens: 12,
  // Ark (barkd sidecar) demo balances — signet defaults
  ark_sats: 75_000,
  ark_pending_sats: 0,
  ark_onchain_sats: 20_000,
  ark_config: { network: 'signet', ark_server: 'https://ark.signet.2nd.dev', esplora: 'https://esplora.signet.2nd.dev' },
  block_height: 892451,
  transactions: [
    { tx_hash: 'ab12cd34ef5678901234567890abcdef12345678', amount_sats: 2_000_000, direction: 'incoming', num_confirmations: 142, block_height: 892310, time_stamp: Math.floor(Date.now()/1000) - 86400, label: 'Channel funding', total_fees: 0, dest_addresses: [] },
    { tx_hash: 'cd34ef5678901234567890abcdef1234567890ab', amount_sats: 250_000, direction: 'incoming', num_confirmations: 28, block_height: 892420, time_stamp: Math.floor(Date.now()/1000) - 7200, label: 'Faucet deposit', total_fees: 0, dest_addresses: [] },
    { tx_hash: 'ff99ee88dd7766554433221100aabbccddeeff00', amount_sats: 100_000, direction: 'incoming', num_confirmations: 0, block_height: 0, time_stamp: Math.floor(Date.now()/1000) - 600, label: 'Incoming from faucet', total_fees: 0, dest_addresses: [] },
    // Networking profits — one or two per type, with human labels (these labels
    // are intended for the production wallet UI too).
    { tx_hash: '1a2b3c4d5e6f78901234567890abcdef11111111', amount_sats: 480_000, direction: 'incoming', num_confirmations: 64, block_height: 892387, time_stamp: Math.floor(Date.now()/1000) - 18000, label: 'Content sale · “Lightning Demo.mp4”', profit_type: 'content_sales', total_fees: 0, dest_addresses: [] },
    { tx_hash: '2b3c4d5e6f7890abcdef1234567890abcd222222', amount_sats: 312_500, direction: 'incoming', num_confirmations: 110, block_height: 892341, time_stamp: Math.floor(Date.now()/1000) - 43200, label: 'Content sale · “Sovereign Computing.pdf”', profit_type: 'content_sales', total_fees: 0, dest_addresses: [] },
    { tx_hash: '3c4d5e6f7890abcdef1234567890abcdef333333', amount_sats: 1_842, direction: 'incoming', num_confirmations: 12, block_height: 892439, time_stamp: Math.floor(Date.now()/1000) - 3600, label: 'Lightning routing fee', profit_type: 'routing_fees', total_fees: 0, dest_addresses: [] },
    { tx_hash: '4d5e6f7890abcdef1234567890abcdef44444444', amount_sats: 906, direction: 'incoming', num_confirmations: 5, block_height: 892446, time_stamp: Math.floor(Date.now()/1000) - 1500, label: 'Lightning routing fee', profit_type: 'routing_fees', total_fees: 0, dest_addresses: [] },
    { tx_hash: '5e6f7890abcdef1234567890abcdef5555555555', amount_sats: 64_200, direction: 'incoming', num_confirmations: 38, block_height: 892410, time_stamp: Math.floor(Date.now()/1000) - 9000, label: 'File relay reward', profit_type: 'relay', total_fees: 0, dest_addresses: [] },
    { tx_hash: '6f7890abcdef1234567890abcdef666666666666', amount_sats: 28_750, direction: 'incoming', num_confirmations: 21, block_height: 892428, time_stamp: Math.floor(Date.now()/1000) - 5400, label: 'Mesh relay forwarding fee', profit_type: 'relay', total_fees: 0, dest_addresses: [] },
  ],
}
function randomHex(bytes) { return Array.from({length: bytes}, () => Math.floor(Math.random()*256).toString(16).padStart(2,'0')).join('') }

// Tracks when a content invoice was requested, so the demo can leave the QR on
// screen for a couple of seconds before reporting it paid.
const invoiceRequestedAt = new Map()

const SEED_BTCRELAY = {
  settings: {
    enabled_for_peers: true,
    allow_peer_requests: true,
    allow_http: false,
    allow_https: true,
    allow_tor: true,
    selected_peer_pubkey: '03d9b8a8db6b4f4d8b8d04c7a467c101f04c0ecbabc0e29e4dcb812a3b1c5f8f04',
    http_endpoint: '',
    https_endpoint: 'https://relay-' + randomHex(5) + '.example.net/',
    tor_endpoint: 'http://btc-relay-demoabcdefghijklmnop.onion/',
  },
  trusted_nodes: [
    {
      pubkey: '03d9b8a8db6b4f4d8b8d04c7a467c101f04c0ecbabc0e29e4dcb812a3b1c5f8f04',
      onion: 'trustedalphaabcdefghijklmnop.onion',
      name: 'Trusted Alpha',
      relay_approved: true,
    },
    {
      pubkey: '02f6ab6c88037cd527a92f3a016a7bd18bb2ebd91d5a3efb1161481b5cf7d9ea2a',
      onion: 'trustedbetabcdefghijklmnopq.onion',
      name: 'Trusted Beta',
      relay_approved: false,
    },
  ],
  requests: [
    {
      id: 'relay-demo-incoming',
      direction: 'incoming',
      status: 'pending',
      peer_pubkey: '02f6ab6c88037cd527a92f3a016a7bd18bb2ebd91d5a3efb1161481b5cf7d9ea2a',
      peer_onion: 'trustedbetabcdefghijklmnopq.onion',
      peer_name: 'Trusted Beta',
      message: 'Can I use your node for a Wasabi broadcast?',
      created_at: new Date(Date.now() - 12 * 60 * 1000).toISOString(),
      updated_at: new Date(Date.now() - 12 * 60 * 1000).toISOString(),
    },
  ],
}

// User state (simulated file-based storage). Returns a fresh object per session.
// In DEMO the visitor is always treated as fully set up ("existing") so the
// onboarding WIZARD (seed/identity/backup) is never forced by the route guard.
// The welcome INTRO still shows via the frontend's per-day replay gate.
function seedUserState() {
  const mode = DEMO ? 'existing' : DEV_MODE
  switch (mode) {
    case 'setup':
      // Setup mode: user needs to set a password (simple setup, not onboarding).
      return { setupComplete: false, onboardingComplete: false, passwordHash: null }
    case 'onboarding':
      // Password already set; visitor still needs to go through onboarding/intro.
      return { setupComplete: true, onboardingComplete: false, passwordHash: MOCK_PASSWORD }
    case 'existing':
      // Fully set up, just needs to log in.
      return { setupComplete: true, onboardingComplete: true, passwordHash: MOCK_PASSWORD }
    case 'boot':
      // Simulate server startup delay (boot screen), then behave like onboarding.
      return { setupComplete: true, onboardingComplete: false, passwordHash: MOCK_PASSWORD }
    default:
      // Default: fully set up (for UI development).
      return { setupComplete: true, onboardingComplete: true, passwordHash: MOCK_PASSWORD }
  }
}

function seedMockState() {
  return {
    analyticsEnabled: false,
    nodeVisibility: 'discoverable',
    // Mesh assistant config — mirrors the daemon's AssistantConfig defaults
    // (assistant.rs). Disabled by default, trusted-only, no model override.
    assistantConfig: {
      enabled: false,
      model: null,
      trusted_only: true,
      backend: 'ollama',
      allowed_contacts: [],
    },
    // Discovery starts OFF, matching the production default — enabling it in
    // the UI walks through the presence-signing overlay.
    nostrDiscovery: false,
    pendingPeerRequests: [
      {
        id: 'preq-demo-1',
        from_nostr_pubkey: '7e2a9c4b1d8f3a6c5e0b9d2f7a4c1e8b3d6f9a2c5e8b1d4f7a0c3e6b9d2f5a8c',
        from_nostr_npub: 'npub1demo0inbound0request0xyzq',
        from_did: 'did:key:z6MkwPn5xQc8vT2sLb6eRu9dYf3gHa7mJzK1oDiW4nXvE8tq',
        from_name: 'basement-node',
        message: 'Hey — saw your node on the relay, mind if we peer?',
        received_at: new Date(Date.now() - 45 * 60000).toISOString(),
        state: 'pending',
        outbound: false,
      },
      // An outbound request still awaiting an answer, so the demo can exercise
      // federation.cancel-request — the daemon only permits cancelling an
      // outbound request in 'sent' state, so without one the demo's cancel
      // button could only ever produce an error.
      {
        id: 'preq-demo-out-1',
        from_nostr_pubkey: '3c8f1a5d9e2b7c4a6f0d3b8e1a5c9f2d7b4e6a0c3f8d1b5e9a2c7f4d0b6e3a8c',
        from_nostr_npub: 'npub1demo0outbound0request0abcd',
        from_did: 'did:key:z6MkqR3nT8yVb2xLc7fPu4dWe6gJa9mHzN1oKiX5vYtA3sqp',
        from_name: 'harbor-node',
        message: 'Requested peering — waiting on their approval.',
        received_at: new Date(Date.now() - 20 * 60000).toISOString(),
        state: 'sent',
        outbound: true,
      },
    ],
    tollgateEnabled: true,
    tollgatePrice: 21,
    tollgateStepMs: 60000,
    tollgateMinSteps: 5,
    tollgateMint: 'https://mint.archy.demo',
    cashuMints: [
      'https://mint.minibits.cash/Bitcoin',
      'https://stablenut.umint.cash',
      'https://mint.coinos.io',
      'https://8333.space:3338',
    ],
    federations: [
      { federation_id: 'fed1' + randomHex(28), name: 'Archipelago Federation', balance_sats: 180_000 },
      { federation_id: 'fed1' + randomHex(28), name: 'Bitcoin Park Mint', balance_sats: 42_500 },
    ],
    // Networking-profits pricing (mirrors crate::streaming::pricing defaults,
    // with a couple enabled so the demo page looks alive).
    streamingServices: [
      { service_id: 'content-download', name: 'Content Downloads', metric: 'bytes', step_size: 1_048_576, price_per_step: 1, min_steps: 0, enabled: true, description: 'Pay-per-byte content downloads from this node', accepted_mints: [] },
      { service_id: 'federation-sync', name: 'Federation Sync Access', metric: 'milliseconds', step_size: 60_000, price_per_step: 1, min_steps: 5, enabled: false, description: 'Timed access to federation sync endpoint', accepted_mints: [] },
      { service_id: 'api-access', name: 'API Access', metric: 'requests', step_size: 1, price_per_step: 1, min_steps: 10, enabled: false, description: 'Per-request API access for external consumers', accepted_mints: [] },
      { service_id: 'nostr-relay', name: 'Nostr Relay Access', metric: 'milliseconds', step_size: 3_600_000, price_per_step: 10, min_steps: 1, enabled: true, description: 'Timed access to the local Nostr relay', accepted_mints: [] },
    ],
  }
}

console.log(`[Auth] Dev mode: ${DEV_MODE}${DEMO ? ' (DEMO multi-session)' : ''}`)

// Broadcast a data-update patch to the WebSocket clients of the CURRENT session
// only (so demo visitors never see each other's state). Outside a request context
// (e.g. startup) this resolves to the default store, matching single-user mode.
function broadcastUpdate(patch) {
  const message = JSON.stringify({
    rev: Date.now(),
    patch: patch
  })
  currentStore().sockets.forEach(client => {
    if (client.readyState === 1) { // OPEN
      client.send(message)
    }
  })
}

// Track used ports and running containers
const usedPorts = new Set([5959, 8100])
const runningContainers = new Map()

// Predefined port mappings for known apps
const portMappings = {
  'atob': 8102,
  'k484': 8103,
  'amin': 8104,
  'filebrowser': 8083,
  'bitcoin-knots': 8332,
  'electrs': 50001,
  'btcpay-server': 23000,
  'lnd': 8080,
  'mempool': 4080,
  'homeassistant': 8123,
  'grafana': 3000,
  'searxng': 8888,
  'ollama': 11434,
  'nextcloud': 8082,
  'vaultwarden': 8222,
  'jellyfin': 8096,
  'photoprism': 2342,
  'immich': 2283,
  'portainer': 9443,
  'uptime-kuma': 3001,
  'tailscale': 41641,
  'fedimint': 8175,
  'thunderhub': 3010,
  'nostr-rs-relay': 7000,
  'syncthing': 8384,
  'penpot': 9001,
  'nginx-proxy-manager': 8181,
  'indeedhub': 8190,
  'dwn': 3000,
  'tor': 9050,
}

// Auto-assign port for unknown apps (start at 8200, increment)
let nextAutoPort = 8200

// Helper: Query real Docker containers
async function getDockerContainers() {
  if (!docker) return {}
  try {
    const containers = await docker.listContainers({ all: true })
    
    // Map of container names to app IDs
    const containerMapping = {
      'archy-bitcoin': 'bitcoin',
      'archy-btcpay': 'btcpay-server',
      'archy-homeassistant': 'homeassistant',
      'archy-grafana': 'grafana',
      'archy-endurain': 'endurain',
      'archy-fedimint': 'fedimint',
      'archy-morphos': 'morphos-server',
      'archy-lnd': 'lightning-stack',
      'archy-mempool-web': 'mempool',
      'mempool-electrs': 'mempool-electrs',
      'archy-ollama': 'ollama',
      'archy-searxng': 'searxng',
      'archy-penpot-frontend': 'penpot'
    }

    const apps = {}
    
    for (const container of containers) {
      const name = container.Names[0].replace(/^\//, '')
      const appId = containerMapping[name]
      
      if (!appId) continue
      
      const isRunning = container.State === 'running'
      const ports = container.Ports || []
      const hostPort = ports.find(p => p.PublicPort)?.PublicPort || null
      
      // Get app metadata
      const appMetadata = {
        'bitcoin': {
          title: 'Bitcoin Core',
          icon: '/assets/img/app-icons/bitcoin-core.png',
          description: 'Full Bitcoin node implementation'
        },
        'btcpay-server': {
          title: 'BTCPay Server',
          icon: '/assets/img/app-icons/btcpay-server.png',
          description: 'Self-hosted Bitcoin payment processor'
        },
        'homeassistant': {
          title: 'Home Assistant',
          icon: '/assets/img/app-icons/homeassistant.png',
          description: 'Open source home automation platform'
        },
        'grafana': {
          title: 'Grafana',
          icon: '/assets/img/grafana.png',
          description: 'Analytics and monitoring platform'
        },
        'endurain': {
          title: 'Endurain',
          icon: '/assets/img/app-icons/endurain.png',
          description: 'Application platform'
        },
        'fedimint': {
          title: 'Fedimint',
          icon: '/assets/img/app-icons/fedimint.png',
          description: 'Federated Bitcoin mint'
        },
        'morphos-server': {
          title: 'MorphOS Server',
          icon: '/assets/img/app-icons/morphos.png',
          description: 'Server platform'
        },
        'lightning-stack': {
          title: 'Lightning Stack',
          icon: '/assets/img/app-icons/lnd.png',
          description: 'Lightning Network (LND)'
        },
        'mempool': {
          title: 'Mempool',
          icon: '/assets/img/app-icons/mempool.webp',
          description: 'Bitcoin blockchain explorer'
        },
        'mempool-electrs': {
          title: 'Electrs',
          icon: '/assets/img/app-icons/electrumx.webp',
          description: 'Electrum protocol indexer for Bitcoin'
        },
        'ollama': {
          title: 'Ollama',
          icon: '/assets/img/app-icons/ollama.png',
          description: 'Run large language models locally'
        },
        'searxng': {
          title: 'SearXNG',
          icon: '/assets/img/app-icons/searxng.png',
          description: 'Privacy-respecting metasearch engine'
        },
        'penpot': {
          title: 'Penpot',
          icon: '/assets/img/app-icons/archipelago-a.svg',
          description: 'Open-source design and prototyping'
        }
      }

      const metadata = appMetadata[appId] || {
        title: appId,
        icon: '/assets/icon/pwa-192x192-v2.png',
        description: `${appId} application`
      }

      apps[appId] = {
        title: metadata.title,
        version: '1.0.0',
        status: isRunning ? 'running' : 'stopped',
        state: isRunning ? 'running' : 'stopped',
        health: isRunning ? 'healthy' : null,
        'static-files': {
          license: 'MIT',
          instructions: metadata.description,
          icon: metadata.icon
        },
        manifest: {
          id: appId,
          title: metadata.title,
          version: '1.0.0',
          description: {
            short: metadata.description,
            long: metadata.description
          },
          'release-notes': 'Initial release',
          license: 'MIT',
          'wrapper-repo': '#',
          'upstream-repo': '#',
          'support-site': '#',
          'marketing-site': '#',
          'donation-url': null,
          interfaces: hostPort ? {
            main: {
              name: 'Web Interface',
              description: `${metadata.title} web interface`,
              ui: true
            }
          } : {}
        },
        installed: {
          'current-dependents': {},
          'current-dependencies': {},
          'last-backup': null,
          'interface-addresses': hostPort ? {
            main: {
              'tor-address': `${appId}.onion`,
              'lan-address': `http://localhost:${hostPort}`
            }
          } : {},
          status: isRunning ? 'running' : 'stopped'
        }
      }
    }
    
    return apps
  } catch (error) {
    console.error('[Docker] Error querying containers:', error.message)
    return {}
  }
}

// Helper: Check if Docker/Podman is available
async function isContainerRuntimeAvailable() {
  try {
    // Try Podman first (Archipelago's choice)
    await execPromise('podman ps')
    return { available: true, runtime: 'podman' }
  } catch {
    try {
      // Fallback to Docker
      await execPromise('docker ps')
      return { available: true, runtime: 'docker' }
    } catch {
      return { available: false, runtime: null }
    }
  }
}

// Marketplace metadata lookup for install (title, description, icon, version)
const marketplaceMetadata = {
  'bitcoin-knots': { title: 'Bitcoin Knots', shortDesc: 'Full Bitcoin node — validate and relay blocks and transactions', icon: '/assets/img/app-icons/bitcoin-knots.webp' },
  'electrs': { title: 'Electrs', shortDesc: 'Electrum protocol indexer for Bitcoin', icon: '/assets/img/app-icons/electrumx.webp' },
  'btcpay-server': { title: 'BTCPay Server', shortDesc: 'Self-hosted Bitcoin payment processor', icon: '/assets/img/app-icons/btcpay-server.png' },
  'lnd': { title: 'LND', shortDesc: 'Lightning Network Daemon', icon: '/assets/img/app-icons/lnd.png' },
  'mempool': { title: 'Mempool Explorer', shortDesc: 'Bitcoin blockchain and mempool visualizer', icon: '/assets/img/app-icons/mempool.webp' },
  'homeassistant': { title: 'Home Assistant', shortDesc: 'Open-source home automation platform', icon: '/assets/img/app-icons/homeassistant.png' },
  'grafana': { title: 'Grafana', shortDesc: 'Analytics and monitoring dashboards', icon: '/assets/img/app-icons/grafana.png' },
  'searxng': { title: 'SearXNG', shortDesc: 'Privacy-respecting metasearch engine', icon: '/assets/img/app-icons/searxng.png' },
  'ollama': { title: 'Ollama', shortDesc: 'Run large language models locally', icon: '/assets/img/app-icons/ollama.png' },
  'penpot': { title: 'Penpot', shortDesc: 'Open-source design and prototyping platform', icon: '/assets/img/app-icons/archipelago-a.svg' },
  'nextcloud': { title: 'Nextcloud', shortDesc: 'Self-hosted cloud storage and collaboration', icon: '/assets/img/app-icons/nextcloud.webp' },
  'vaultwarden': { title: 'Vaultwarden', shortDesc: 'Self-hosted password manager (Bitwarden-compatible)', icon: '/assets/img/app-icons/vaultwarden.webp' },
  'jellyfin': { title: 'Jellyfin', shortDesc: 'Free media server for movies, music, and photos', icon: '/assets/img/app-icons/jellyfin.webp' },
  'photoprism': { title: 'PhotoPrism', shortDesc: 'AI-powered photo management', icon: '/assets/img/app-icons/photoprism.svg' },
  'immich': { title: 'Immich', shortDesc: 'High-performance photo and video backup', icon: '/assets/img/app-icons/immich.png' },
  'filebrowser': { title: 'File Browser', shortDesc: 'Web-based file manager', icon: '/assets/img/app-icons/file-browser.webp' },
  'nginx-proxy-manager': { title: 'Nginx Proxy Manager', shortDesc: 'Easy proxy management with SSL', icon: '/assets/img/app-icons/nginx.svg' },
  'portainer': { title: 'Portainer', shortDesc: 'Container management UI', icon: '/assets/img/app-icons/portainer.webp' },
  'uptime-kuma': { title: 'Uptime Kuma', shortDesc: 'Self-hosted monitoring tool', icon: '/assets/img/app-icons/uptime-kuma.webp' },
  'tailscale': { title: 'Tailscale', shortDesc: 'Zero-config VPN for secure remote access', icon: '/assets/img/app-icons/tailscale.webp' },
  'fedimint': { title: 'Fedimint', shortDesc: 'Federated Bitcoin mint with Guardian UI', icon: '/assets/img/app-icons/fedimint.png' },
  'indeedhub': { title: 'Indeehub', shortDesc: 'Bitcoin documentary streaming platform', icon: '/assets/img/app-icons/indeedhub.png' },
  'dwn': { title: 'Decentralized Web Node', shortDesc: 'Store and sync personal data with DID-based access', icon: '/assets/img/app-icons/dwn.svg' },
  'nostr-rs-relay': { title: 'Nostr Relay', shortDesc: 'Run your own Nostr relay', icon: '/assets/img/app-icons/nostrudel.svg' },
  'syncthing': { title: 'Syncthing', shortDesc: 'Peer-to-peer file synchronization', icon: '/assets/img/app-icons/archipelago-a.svg' },
  'thunderhub': { title: 'ThunderHub', shortDesc: 'Lightning node management UI with channel management and payments', icon: '/assets/img/app-icons/thunderhub.svg' },
  'tor': { title: 'Tor', shortDesc: 'Anonymous communication over the Tor network', icon: '/assets/img/app-icons/tor.svg' },
  'amin': { title: 'Amin', shortDesc: 'Administrative interface for Archipelago', icon: '/assets/icon/pwa-192x192-v2.png' },
}

// Helper: Install package with container runtime (if available) or simulate
async function installPackage(id, manifestUrl, opts = {}) {
  console.log(`[Package] 📦 Installing ${id}...`)

  try {
    // Check if already installed
    if (mockData['package-data'][id]) {
      throw new Error(`Package ${id} is already installed`)
    }

    const version = opts.version || '0.1.0'
    const runtime = await isContainerRuntimeAvailable()

    // Get package metadata from marketplace lookup, then fallback
    const metadata = marketplaceMetadata[id] || {
      title: id.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '),
      shortDesc: `${id} application`,
      icon: `/assets/img/app-icons/${id}.png`
    }

    // Determine port — use known mapping, or auto-assign a unique one
    let assignedPort = portMappings[id]
    if (!assignedPort) {
      while (usedPorts.has(nextAutoPort)) nextAutoPort++
      assignedPort = nextAutoPort++
    }
    usedPorts.add(assignedPort)
    
    let containerMode = false
    let actuallyRunning = false
    
    // Try to run with container runtime if available
    if (runtime.available) {
      try {
        console.log(`[Package] 🐳 ${runtime.runtime} available, attempting to run container...`)
        
        const containerName = `${id}-archipelago`
        const stopCmd = runtime.runtime === 'podman' 
          ? `podman stop ${containerName} 2>/dev/null || true`
          : `docker stop ${containerName} 2>/dev/null || true`
        const rmCmd = runtime.runtime === 'podman'
          ? `podman rm ${containerName} 2>/dev/null || true`
          : `docker rm ${containerName} 2>/dev/null || true`
        
        // Stop and remove existing container if it exists
        await execPromise(stopCmd)
        await execPromise(rmCmd)
        
        // Check if image exists
        const imageCheckCmd = runtime.runtime === 'podman'
          ? `podman images -q ${id}:${version}`
          : `docker images -q ${id}:${version}`
        
        let { stdout } = await execPromise(imageCheckCmd)
        
        if (stdout.trim()) {
          // Image exists, start container
          const runCmd = runtime.runtime === 'podman'
            ? `podman run -d --name ${containerName} -p ${assignedPort}:80 ${id}:${version}`
            : `docker run -d --name ${containerName} -p ${assignedPort}:80 ${id}:${version}`
          
          await execPromise(runCmd)
          
          // Wait for container to be ready
          await new Promise(resolve => setTimeout(resolve, 2000))
          
          // Verify container is running
          const statusCmd = runtime.runtime === 'podman'
            ? `podman ps --filter name=${containerName} --format "{{.Status}}"`
            : `docker ps --filter name=${containerName} --format "{{.Status}}"`
          
          const { stdout: containerStatus } = await execPromise(statusCmd)
          
          if (containerStatus.includes('Up')) {
            containerMode = true
            actuallyRunning = true
            runningContainers.set(id, { 
              port: assignedPort, 
              containerId: containerName, 
              runtime: runtime.runtime 
            })
            console.log(`[Package] 🐳 ${runtime.runtime} container running on port ${assignedPort}`)
          }
        } else {
          console.log(`[Package] ℹ️  Container image ${id}:${version} not found, using simulation mode`)
        }
      } catch (containerError) {
        console.log(`[Package] ⚠️  Container error (${containerError.message}), falling back to simulation`)
      }
    } else {
      console.log(`[Package] ℹ️  Container runtime not available, using simulation mode`)
    }
    
    // If container didn't work, simulate installation
    if (!containerMode) {
      await new Promise(resolve => setTimeout(resolve, 1500))
      runningContainers.set(id, { port: assignedPort, containerId: null, runtime: null })
    }
    
    // Add to mock data using staticApp format for consistency
    mockData['package-data'][id] = {
      ...staticApp({
        id,
        title: metadata.title,
        version,
        shortDesc: metadata.shortDesc,
        longDesc: metadata.shortDesc,
        state: 'running',
        lanPort: assignedPort,
        icon: metadata.icon,
      }),
      port: assignedPort,
      containerMode: containerMode,
      actuallyRunning: actuallyRunning,
    }
    
    // Broadcast update
    broadcastUpdate([
      {
        op: 'add',
        path: `/package-data/${id}`,
        value: mockData['package-data'][id]
      }
    ])
    
    if (containerMode) {
      console.log(`[Package] ✅ ${id} installed and RUNNING at http://localhost:${assignedPort}`)
    } else {
      console.log(`[Package] ✅ ${id} installed (simulated)`)
    }
    
    return { success: true, port: assignedPort, containerMode }
    
  } catch (error) {
    console.error(`[Package] ❌ Installation failed:`, error.message)
    throw error
  }
}

// Helper: Uninstall package
async function uninstallPackage(id) {
  console.log(`[Package] 🗑️  Uninstalling ${id}...`)
  
  try {
    if (staticDevApps[id]) {
      throw new Error(`${id} is a demo app and cannot be uninstalled`)
    }
    if (!mockData['package-data'][id]) {
      throw new Error(`Package ${id} is not installed`)
    }
    
    // Stop container if it's running
    const containerInfo = runningContainers.get(id)
    if (containerInfo && containerInfo.containerId) {
      try {
        const runtime = containerInfo.runtime || 'docker'
        const stopCmd = runtime === 'podman'
          ? `podman stop ${containerInfo.containerId} 2>/dev/null || true`
          : `docker stop ${containerInfo.containerId} 2>/dev/null || true`
        const rmCmd = runtime === 'podman'
          ? `podman rm ${containerInfo.containerId} 2>/dev/null || true`
          : `docker rm ${containerInfo.containerId} 2>/dev/null || true`
        
        console.log(`[Package] 🐳 Stopping container ${containerInfo.containerId}...`)
        await execPromise(stopCmd)
        await execPromise(rmCmd)
        console.log(`[Package] 🐳 Container stopped`)
      } catch (error) {
        console.log(`[Package] ⚠️  Error stopping container: ${error.message}`)
      }
    }
    
    await new Promise(resolve => setTimeout(resolve, 1000))
    
    const port = mockData['package-data'][id].port
    if (port) {
      usedPorts.delete(port)
    }
    
    runningContainers.delete(id)
    delete mockData['package-data'][id]
    
    broadcastUpdate([
      {
        op: 'remove',
        path: `/package-data/${id}`
      }
    ])
    
    console.log(`[Package] ✅ ${id} uninstalled successfully`)
    return { success: true }
    
  } catch (error) {
    console.error(`[Package] ❌ Uninstall failed:`, error.message)
    throw error
  }
}

// Mock data
const SEED_MOCKDATA = {
  'server-info': {
    id: 'archipelago-demo',
    version: APP_VERSION,
    name: 'Archipelago',
    pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
    'status-info': {
      restarting: false,
      'shutting-down': false,
      updated: false,
      'backup-progress': null,
      'update-progress': null,
      // The mock has no container scanner — report the scan as done so app
      // cards never sit on "Checking..." in demo/dev.
      'containers-scanned': true,
    },
    'lan-address': 'localhost',
    'tor-address': 'archydemox7k3pnw4hv5qz2jcbr6dwefys3ockqzf4mzjlvxot2ioad.onion',
    unread: 3,
    'wifi-ssids': ['Home-5G', 'Archipelago-Mesh', 'Neighbors-Open'],
    'zram-enabled': true,
  },
  'package-data': {},  // Will be populated from Docker + static apps
  ui: {
    name: 'Archipelago',
    'ack-welcome': '0.1.0',
    marketplace: {
      'selected-hosts': [],
      'known-hosts': {},
    },
    theme: 'dark',
  },
}

// Helper to build a static app entry
function staticApp({ id, title, version, shortDesc, longDesc, license, state, lanPort, torHost, icon, noUi }) {
  return {
    title,
    version,
    status: state,
    state,
    'static-files': {
      license: license || 'MIT',
      instructions: shortDesc,
      icon: icon || `/assets/img/app-icons/${id}.png`,
    },
    manifest: {
      id,
      title,
      version,
      description: { short: shortDesc, long: longDesc || shortDesc },
      'release-notes': 'Latest stable release',
      license: license || 'MIT',
      'wrapper-repo': '#',
      'upstream-repo': '#',
      'support-site': '#',
      'marketing-site': '#',
      'donation-url': null,
      // ui:null mirrors the real backend's ui_detection verdict for headless
      // services (databases/APIs/workers): address present, no Launch button,
      // files under the Services tab.
      interfaces: {
        main: noUi
          ? { name: 'Service', description: `${title} backend service`, ui: null }
          : { name: 'Web Interface', description: `${title} web interface`, ui: true },
      },
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': torHost ? `${torHost}.onion` : `${id}.onion`,
          'lan-address': lanPort ? `http://localhost:${lanPort}` : '',
        },
      },
      status: state,
    },
  }
}

// Static dev apps (always shown in My Apps when using mock backend).
const staticDevApps = {
  bitcoin: staticApp({
    id: 'bitcoin',
    title: 'Bitcoin Core',
    version: '28.4.0',
    shortDesc: 'Full Bitcoin node',
    longDesc: 'Validate every transaction and block. Full consensus enforcement — the bedrock of sovereignty.',
    state: 'running',
    lanPort: 8332,
    icon: '/assets/img/app-icons/bitcoin-core.png',
  }),
  'bitcoin-knots': staticApp({
    id: 'bitcoin-knots',
    title: 'Bitcoin Knots',
    version: '28.1.0',
    shortDesc: 'Full Bitcoin node',
    longDesc: 'Validate and relay Bitcoin blocks and transactions with the Archipelago custom node UI.',
    state: 'running',
    lanPort: 8334,
    icon: '/assets/img/app-icons/bitcoin-knots.webp',
  }),
  lnd: staticApp({
    id: 'lnd',
    title: 'LND',
    version: '0.18.3',
    shortDesc: 'Lightning Network Daemon',
    longDesc: 'Instant Bitcoin payments with near-zero fees. Open channels, route payments, earn sats.',
    state: 'running',
    lanPort: 8080,
    icon: '/assets/img/app-icons/lnd.png',
  }),
  electrumx: staticApp({
    id: 'electrumx',
    title: 'ElectrumX',
    version: '1.18.0',
    shortDesc: 'Electrum server',
    longDesc: 'Private blockchain indexing for wallet lookups. Connect Sparrow, BlueWallet, or any Electrum-compatible wallet.',
    state: 'running',
    lanPort: 50002,
    icon: '/assets/img/app-icons/electrumx.webp',
  }),
  mempool: staticApp({
    id: 'mempool',
    title: 'Mempool',
    version: '3.0.0',
    shortDesc: 'Blockchain explorer & fee estimator',
    longDesc: 'Real-time mempool visualization, transaction tracking, and fee estimation — all on your own node.',
    license: 'AGPL-3.0',
    state: 'running',
    lanPort: 4080,
    icon: '/assets/img/app-icons/mempool.webp',
  }),
  lorabell: staticApp({
    id: 'lorabell',
    title: 'LoraBell',
    version: '1.0.0',
    shortDesc: 'LoRa doorbell',
    longDesc: 'Receive doorbell notifications over LoRa radio — no WiFi or internet required.',
    state: 'running',
    lanPort: null,
  }),
  meshtastic: staticApp({
    id: 'meshtastic',
    title: 'Meshtastic',
    version: '2-daily-alpine',
    shortDesc: 'LoRa mesh networking',
    longDesc: 'Open-source mesh networking for LoRa radios. Create decentralized communication networks.',
    state: 'running',
    lanPort: 4403,
    icon: '/assets/img/app-icons/meshcore.svg',
  }),
  // Self-deployed compose stack (v1.7.117+ services classification demo):
  // the main app serves a real web page → launchable; its media backend has
  // an exposed port but no UI → Services tab, no Launch button.
  podsteadr: staticApp({
    id: 'podsteadr',
    title: 'Podsteadr',
    version: '0.4.1',
    shortDesc: 'Self-hosted podcast studio',
    longDesc: 'Record, host, and publish podcasts from your own node — deployed by hand outside the app store to demo third-party stacks.',
    state: 'running',
    lanPort: 8095,
  }),
  'podsteadr-mediamtx': staticApp({
    id: 'podsteadr-mediamtx',
    title: 'Podsteadr MediaMTX',
    version: '1.9.3',
    shortDesc: 'RTMP/WebRTC media backend',
    longDesc: 'Streaming backend for Podsteadr. Speaks RTMP and WebRTC on exposed ports — no web page of its own.',
    state: 'running',
    lanPort: 8889,
    noUi: true,
  }),
  filebrowser: staticApp({
    id: 'filebrowser',
    title: 'File Browser',
    version: '2.27.0',
    shortDesc: 'Web-based file manager',
    longDesc: 'Browse, upload, and manage files through an elegant web interface. Drag-and-drop uploads, media previews, and sharing.',
    state: 'running',
    lanPort: 8083,
    icon: '/assets/img/app-icons/file-browser.webp',
  }),
  fedimint: staticApp({
    id: 'fedimint',
    title: 'Fedimint',
    version: '0.10.0',
    shortDesc: 'Federated Bitcoin mint',
    longDesc: 'Federated Chaumian e-cash mint with Guardian UI. Community custody, private payments, and Lightning gateways.',
    state: 'running',
    lanPort: 8175,
    icon: '/assets/img/app-icons/fedimint.png',
  }),
  'btcpay-server': staticApp({
    id: 'btcpay-server',
    title: 'BTCPay Server',
    version: '2.0.4',
    shortDesc: 'Self-hosted Bitcoin payment processor',
    longDesc: 'Accept Bitcoin payments in your store — no fees, no middlemen, no KYC.',
    state: 'running',
    lanPort: 23000,
    icon: '/assets/img/app-icons/btcpay-server.png',
  }),
  grafana: staticApp({
    id: 'grafana',
    title: 'Grafana',
    version: '11.4.0',
    shortDesc: 'Analytics and monitoring dashboards',
    longDesc: 'Visualize node metrics, Bitcoin sync progress, and Lightning routing on custom dashboards.',
    license: 'AGPL-3.0',
    state: 'running',
    lanPort: 3000,
    icon: '/assets/img/app-icons/grafana.png',
  }),
  nextcloud: staticApp({
    id: 'nextcloud',
    title: 'Nextcloud',
    version: '29.0.0',
    shortDesc: 'Self-hosted cloud storage and collaboration',
    longDesc: 'Files, calendar, contacts, and collaboration — replace Google Drive and Dropbox entirely.',
    license: 'AGPL-3.0',
    state: 'running',
    lanPort: 8082,
    icon: '/assets/img/app-icons/nextcloud.webp',
  }),
  jellyfin: staticApp({
    id: 'jellyfin',
    title: 'Jellyfin',
    version: '10.10.3',
    shortDesc: 'Free media server for movies, music, and photos',
    longDesc: 'Stream your media collection to any device — no subscription, no tracking.',
    license: 'GPL-2.0',
    state: 'running',
    lanPort: 8096,
    icon: '/assets/img/app-icons/jellyfin.webp',
  }),
  vaultwarden: staticApp({
    id: 'vaultwarden',
    title: 'Vaultwarden',
    version: '1.32.5',
    shortDesc: 'Self-hosted password manager',
    longDesc: 'Bitwarden-compatible password manager. Keep your credentials under your own roof.',
    license: 'AGPL-3.0',
    state: 'running',
    lanPort: 8222,
    icon: '/assets/img/app-icons/vaultwarden.webp',
  }),
  'nostr-rs-relay': staticApp({
    id: 'nostr-rs-relay',
    title: 'Nostr Relay',
    version: '0.9.0',
    shortDesc: 'Run your own Nostr relay',
    longDesc: 'Sovereign social networking — publish notes and serve your community, censorship-free.',
    state: 'running',
    lanPort: 7000,
    icon: '/assets/img/app-icons/nostrudel.svg',
  }),
  searxng: staticApp({
    id: 'searxng',
    title: 'SearXNG',
    version: '2024.12.16',
    shortDesc: 'Privacy-respecting metasearch engine',
    longDesc: 'Aggregate search results from dozens of engines with zero tracking.',
    license: 'AGPL-3.0',
    state: 'running',
    lanPort: 8888,
    icon: '/assets/img/app-icons/searxng.png',
  }),
  'uptime-kuma': staticApp({
    id: 'uptime-kuma',
    title: 'Uptime Kuma',
    version: '1.23.16',
    shortDesc: 'Self-hosted monitoring tool',
    longDesc: 'Monitor uptime for your services with alerts and status pages.',
    state: 'running',
    lanPort: 3001,
    icon: '/assets/img/app-icons/uptime-kuma.webp',
  }),
  // Pre-installed in the demo so the flagship media app is one click away.
  // The demo launch bypasses /app/indeedhub/ entirely — the iframe loads the
  // :2101 whole-origin proxy of the real site (see useDemoIntro.demoAppUrl
  // and the nginx-demo.conf :2101 server block).
  indeedhub: staticApp({
    id: 'indeedhub',
    title: 'Indeehub',
    version: '1.0.0',
    shortDesc: 'Bitcoin documentary streaming platform',
    longDesc: 'Stream Bitcoin documentaries and community media — Nostr-native publishing, zaps, and playlists on your own node.',
    state: 'running',
    lanPort: 8190,
    icon: '/assets/img/app-icons/indeedhub.png',
  }),
}

function mergePackageData(dockerApps) {
  return { ...dockerApps, ...staticDevApps }
}

// Initialize package data from Docker on startup
async function initializePackageData() {
  console.log('[Docker] Querying running containers...')
  const dockerApps = await getDockerContainers()
  mockData['package-data'] = mergePackageData(dockerApps)
  
  const appCount = Object.keys(mockData['package-data']).length
  const runningCount = Object.values(mockData['package-data']).filter(app => app.state === 'running').length
  
  console.log(`[Docker] Found ${appCount} containers (${runningCount} running)`)
  
  if (appCount > 0) {
    console.log('[Docker] Apps detected:')
    Object.entries(mockData['package-data']).forEach(([id, app]) => {
      const port = app.installed?.['interface-addresses']?.main?.['lan-address']
      console.log(`  - ${app.title} (${app.state})${port ? ` → ${port}` : ''}`)
    })
  } else {
    console.log('[Docker] No containers found. Start docker-compose to see apps.')
  }
}

// Handle CORS preflight
app.options('/rpc/v1', (req, res) => {
  res.status(200).end()
})

app.get('/', (_req, res) => {
  const uiPort = process.env.VITE_DEV_SERVER_PORT || '8102'
  res
    .status(200)
    .type('html')
    .send(`<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <meta http-equiv="refresh" content="0; url=http://localhost:${uiPort}/">
    <title>Archipelago dev backend</title>
  </head>
  <body>
    <p>This is the mock JSON-RPC backend. Open the dashboard at
      <a href="http://localhost:${uiPort}/">http://localhost:${uiPort}/</a>.
    </p>
  </body>
</html>`)
})

app.get('/rpc/v1', (_req, res) => {
  const uiPort = process.env.VITE_DEV_SERVER_PORT || '8102'
  res
    .status(405)
    .type('text/plain')
    .send(`JSON-RPC is available at /rpc/v1 for POST requests only.\nOpen the dashboard at http://localhost:${uiPort}/.\n`)
})

// DEMO runs on a testnet (signet) so visitors can play with worthless coins.
const DEMO_CHAIN = DEMO ? 'signet' : 'main'

function mockBitcoinBlockchainInfo() {
  return {
    chain: DEMO_CHAIN,
    blocks: 902418,
    headers: 902418,
    bestblockhash: randomHex(32),
    difficulty: 126_984_812_384_099.3,
    verificationprogress: 0.999998,
    initialblockdownload: false,
    size_on_disk: 678_000_000_000,
    pruned: false,
    chainwork: '00000000000000000000000000000000000000008d4b42d8b9b0000000000000',
  }
}

function mockBitcoinNetworkInfo() {
  return {
    version: 270100,
    subversion: '/Satoshi:27.1/Knots:20240513/',
    protocolversion: 70016,
    localservices: '0000000000000409',
    localrelay: true,
    timeoffset: 0,
    networkactive: true,
    connections: 18,
    networks: [
      { name: 'ipv4', limited: false, reachable: true, proxy: '', proxy_randomize_credentials: false },
      { name: 'onion', limited: false, reachable: true, proxy: '127.0.0.1:9050', proxy_randomize_credentials: true },
    ],
  }
}

// One monitoring MetricSnapshot in the exact shape Monitoring.vue expects:
// epoch-second timestamps, nested system metrics, per-container stats with
// limits + block IO, rpc_latency_ms and ws_connections at the top level.
function monitoringSnapshot(tsSecs, seed = 0) {
  const wave = Math.sin((tsSecs / 60 + seed) / 6)
  const mkContainer = (name, cpuPct, mem, limit) => ({
    name,
    cpu_percent: cpuPct,
    mem_used_bytes: mem,
    mem_limit_bytes: limit,
    net_rx_bytes: Math.floor(Math.random() * 4_000_000),
    net_tx_bytes: Math.floor(Math.random() * 2_500_000),
    block_read_bytes: Math.floor(Math.random() * 9_000_000),
    block_write_bytes: Math.floor(Math.random() * 5_000_000),
  })
  return {
    timestamp: tsSecs,
    system: {
      cpu_percent: +(14 + wave * 6 + Math.random() * 8).toFixed(1),
      mem_used_bytes: Math.floor(6_000_000_000 + wave * 600_000_000 + Math.random() * 300_000_000),
      mem_total_bytes: 16_000_000_000,
      disk_used_bytes: 620_000_000_000,
      disk_total_bytes: 1_800_000_000_000,
      net_rx_bytes: Math.floor(2_000_000 + Math.random() * 6_000_000),
      net_tx_bytes: Math.floor(1_000_000 + Math.random() * 4_000_000),
      load_avg_1: +(0.5 + Math.random() * 1.2).toFixed(2),
      load_avg_5: +(0.7 + Math.random() * 0.9).toFixed(2),
      load_avg_15: +(0.6 + Math.random() * 0.7).toFixed(2),
    },
    containers: [
      mkContainer('bitcoin-knots', +(6 + Math.random() * 5).toFixed(1), 1_200_000_000, 4_000_000_000),
      mkContainer('lnd', +(2 + Math.random() * 3).toFixed(1), 480_000_000, 2_000_000_000),
      mkContainer('electrs', +(9 + Math.random() * 7).toFixed(1), 890_000_000, 2_000_000_000),
      mkContainer('mempool', +(3 + Math.random() * 4).toFixed(1), 320_000_000, 1_000_000_000),
      mkContainer('filebrowser', +(0.4 + Math.random()).toFixed(1), 45_000_000, 512_000_000),
    ],
    rpc_latency_ms: +(2 + Math.random() * 9).toFixed(1),
    ws_connections: 2,
  }
}

function bitcoinRelayStatusPayload() {
  return {
    settings: bitcoinRelayMockState.settings,
    trusted_nodes: bitcoinRelayMockState.trusted_nodes,
    requests: bitcoinRelayMockState.requests,
    local_node: {
      synced: true,
      blocks: 902418,
      headers: 902418,
      chain: DEMO_CHAIN,
      status_ok: true,
      status_stale: false,
      error: null,
    },
    credentials: {
      username: 'txrelay',
      available: true,
      password_available: true,
      rpcauth_available: true,
      client_env_available: true,
      client_env_path: '/var/lib/archipelago/secrets/bitcoin-rpc-txrelay-client.env',
      restart_hint: 'If this was just generated, restart Bitcoin Core/Knots so bitcoind loads the txrelay rpcauth whitelist.',
    },
  }
}

app.get('/app/bitcoin-ui/', async (_req, res) => {
  try {
    const html = await fs.readFile(path.join(__dirname, '..', 'docker', 'bitcoin-ui', 'index.html'), 'utf8')
    res.type('html').send(html)
  } catch (error) {
    res.status(500).type('text/plain').send(`Unable to load Bitcoin UI mock: ${error.message}`)
  }
})

// ── Mock app UIs served in the in-app iframe (DEMO) ─────────────────────────
function demoAppShell(title, sub, iconPath, bodyHtml) {
  const accent = '#f7931a' // Archipelago orange
  return `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>${title}</title>
<style>
  *{margin:0;padding:0;box-sizing:border-box}
  body{background:#000;color:#f2f2f4;font-family:system-ui,-apple-system,Segoe UI,Roboto,sans-serif;min-height:100vh;padding:28px;
       background-image:radial-gradient(1200px 500px at 50% -10%, rgba(247,147,26,.07), transparent 70%);}
  .wrap{max-width:860px;margin:0 auto}
  .hd{display:flex;align-items:center;gap:14px;margin-bottom:22px}
  .ico{width:46px;height:46px;border-radius:12px;object-fit:cover;background:rgba(255,255,255,.04);border:1px solid rgba(255,255,255,.08);flex-shrink:0}
  h1{font-size:22px;font-weight:650}
  .sub{color:#8a8f9a;font-size:13px;margin-top:2px}
  .grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:14px;margin-bottom:18px}
  .card{background:rgba(255,255,255,.035);border:1px solid rgba(255,255,255,.08);border-radius:14px;padding:16px}
  .k{color:#8a8f9a;font-size:12px;text-transform:uppercase;letter-spacing:.5px}
  .v{font-size:20px;font-weight:650;margin-top:6px}
  .v.mono{font-family:ui-monospace,Menlo,monospace;font-size:13px;word-break:break-all;font-weight:500}
  .bar{height:8px;border-radius:6px;background:rgba(255,255,255,.07);overflow:hidden;margin-top:10px}
  .bar>i{display:block;height:100%;background:${accent}}
  .badge{display:inline-flex;align-items:center;gap:6px;background:rgba(34,197,94,.14);color:#86efac;border:1px solid rgba(34,197,94,.3);padding:4px 10px;border-radius:999px;font-size:12px}
  table{width:100%;border-collapse:collapse;font-size:13px}
  td,th{text-align:left;padding:9px 6px;border-bottom:1px solid rgba(255,255,255,.06)}
  th{color:#8a8f9a;font-weight:500;font-size:11px;text-transform:uppercase}
  .demo-tag{position:fixed;bottom:14px;right:16px;font-size:11px;color:#5b6070}
</style></head><body><div class="wrap">
  <div class="hd">
    <img class="ico" src="${iconPath}" onerror="this.style.visibility='hidden'" alt="">
    <div><h1>${title}</h1><div class="sub">${sub}</div></div>
  </div>
  ${bodyHtml}
</div><div class="demo-tag">Archipelago demo · signet</div></body></html>`
}

// ── Rich peer content library (demo) ─────────────────────────────────────────
// What a healthy decentralised network's shared catalog looks like: films,
// series, books, music, photography and documents with real metadata. Music,
// photos and documents are backed by REAL committed files (they play/open);
// films/series/books are paid so the buy-flow poster/cover previews carry the
// visuals. Photographs in demo/peer-media are real free-license images from
// Wikimedia Commons (per-file credit in each description); the two Zazawowow
// tracks are real songs published on Wavlake (wavlake.com), bytes + artwork
// committed from the public catalog CDN.
const PEER_MEDIA = (f) => path.join(__dirname, '..', 'demo', 'peer-media', f)
const PEER_CONTENT = (...p) => path.join(__dirname, '..', 'demo', 'content', ...p)
const PEER_LIBRARY = [
  // Films & series — paid, poster previews
  { id: 'film-block-height', filename: 'Block Height (2025) 2160p.mp4', mime_type: 'video/mp4', size_bytes: 4_512_374_784, access: { paid: { price_sats: 2500 } }, preview: PEER_MEDIA('poster-film-block.jpg'),
    description: 'Documentary · 2025 · 1h 42m · 3840×2160 HEVC 10-bit · dir. M. Castillo — three node operators ride out a halving epoch.' },
  { id: 'film-the-signal', filename: 'The Signal (2024) 1080p.mkv', mime_type: 'video/x-matroska', size_bytes: 2_147_483_648, access: { paid: { price_sats: 2100 } }, preview: PEER_MEDIA('poster-film-signal.jpg'),
    description: 'Thriller · 2024 · 2h 08m · 1920×1080 x264 · A radio engineer discovers her mesh network is carrying more than messages.' },
  { id: 'film-meshwork', filename: 'Meshwork — infrastructure for the willing (720p).mp4', mime_type: 'video/mp4', size_bytes: 891_289_600, access: { paid: { price_sats: 800 } }, preview: PEER_MEDIA('poster-film-mesh.jpg'),
    description: 'Documentary short · 2026 · 34m · 1280×720 · Community LoRa + Reticulum builds across the highlands.' },
  { id: 'series-node-runners-s01e01', filename: 'Node Runners S01E01 — Genesis.mkv', mime_type: 'video/x-matroska', size_bytes: 734_003_200, access: { paid: { price_sats: 500 } }, preview: PEER_MEDIA('poster-series-node.jpg'),
    description: 'Series · S01E01 · 43m · 1080p · Docuseries following first-time sovereign node builders.' },
  // Books — paid, cover previews
  { id: 'book-sovereign-notes', filename: 'The Sovereign Individual — annotated.epub', mime_type: 'application/epub+zip', size_bytes: 3_842_048, access: { paid: { price_sats: 300 } }, preview: PEER_MEDIA('cover-book-sovereign.jpg'),
    description: 'EPUB · 512 pp · community margin-notes edition, 2026 · Davidson & Rees-Mogg.' },
  { id: 'book-cypherpunk-essays', filename: 'Cypherpunk Essays Vol. II.epub', mime_type: 'application/epub+zip', size_bytes: 2_097_152, access: { paid: { price_sats: 210 } }, preview: PEER_MEDIA('cover-book-cypher.jpg'),
    description: 'EPUB · 348 pp · 2025 anthology — privacy engineering, remailers, digital cash history.' },
  { id: 'book-broadcast-power', filename: 'Broadcast Power — pirate radio to mesh.pdf', mime_type: 'application/pdf', size_bytes: 18_874_368, access: { paid: { price_sats: 150 } }, preview: PEER_MEDIA('cover-book-broadcast.jpg'),
    description: 'PDF · 214 pp · illustrated 2024 history of community-run transmission networks.' },
  // Music — real committed files. The two Wavlake tracks are real releases by
  // Zazawowow (metadata from catalog.wavlake.com); the first is the demo's
  // showcase PAID track — buying it delivers the actual mp3 and autoplays it.
  { id: 'song-webfive', filename: 'Zazawowow - WEBFIVEFOURTHREETWOONE.mp3', mime_type: 'audio/mpeg', size_bytes: 6_041_903, access: { paid: { price_sats: 21 } }, disk: PEER_CONTENT('music', 'Zazawowow - WEBFIVEFOURTHREETWOONE.mp3'),
    description: 'MP3 128 kbps · WEBFIVE · Zazawowow · 4:09 · published on Wavlake.' },
  { id: 'song-michael-saylor', filename: 'Zazawowow - Michael Michael Saylor.mp3', mime_type: 'audio/mpeg', size_bytes: 4_517_590, access: 'free', disk: PEER_CONTENT('music', 'Zazawowow - Michael Michael Saylor.mp3'),
    description: 'MP3 128 kbps · Michael Michael Saylor (single) · Zazawowow · 3:03 · published on Wavlake.' },
  { id: 'song-leaves-brown', filename: 'All the leaves are brown.mp3', mime_type: 'audio/mpeg', size_bytes: 2_936_012, access: 'free', disk: PEER_CONTENT('music', 'All the leaves are brown.mp3'),
    description: 'MP3 320 kbps · Archy Sessions, 2026 · folk cover, one-take living-room recording.' },
  { id: 'song-exploited-substrate', filename: 'An Exploited Substrate.mp3', mime_type: 'audio/mpeg', size_bytes: 4_194_304, access: 'free', disk: PEER_CONTENT('music', 'An Exploited Substrate.mp3'),
    description: 'MP3 320 kbps · Node Noise EP, 2025 · instrumental synthwave.' },
  { id: 'song-architects', filename: 'Architects of Tomorrow.wav', mime_type: 'audio/wav', size_bytes: 34_734_080, access: 'free', disk: PEER_CONTENT('music', 'Architects of Tomorrow.wav'),
    description: 'WAV 16-bit/44.1 kHz lossless master · Node Noise EP, 2025.' },
  { id: 'song-bitcoin-salad', filename: 'Bitcoin Salad.MP3', mime_type: 'audio/mpeg', size_bytes: 3_984_588, access: 'free', disk: PEER_CONTENT('music', 'Bitcoin Salad.MP3'),
    description: 'MP3 256 kbps · Kitchen Mix Vol. 3, 2024 · novelty electro.' },
  { id: 'song-builders', filename: 'Builders, not talkers (Remastered).mp3', mime_type: 'audio/mpeg', size_bytes: 4_508_876, access: { paid: { price_sats: 100 } }, disk: PEER_CONTENT('music', 'Builders, not talkers (Remastered).mp3'),
    description: 'MP3 320 kbps · 2026 remaster · hip-hop instrumental.' },
  // Photography — free, the committed JPEGs themselves (preview = full image).
  // All real photographs from Wikimedia Commons (free licenses, credited below).
  { id: 'photo-aurora-fjord', filename: 'aurora-over-the-fjord.jpg', mime_type: 'image/jpeg', size_bytes: 482_891, access: 'free', disk: PEER_MEDIA('photo-aurora-fjord.jpg'),
    description: '1920×1280 · Aurora borealis over a Lofoten fjord, snowy peaks mirrored in the sea · Norway · Wikimedia Commons, CC BY 3.0.' },
  { id: 'photo-mountain-lake', filename: 'mountain-lake-still.jpg', mime_type: 'image/jpeg', size_bytes: 680_877, access: 'free', disk: PEER_MEDIA('photo-mountain-lake.jpg'),
    description: '1920×1474 · Lago di Limides at sunset, Dolomite spires reflected in still water · Italy · Wikimedia Commons, CC BY 4.0.' },
  { id: 'photo-city-neon', filename: 'city-neon-rain.jpg', mime_type: 'image/jpeg', size_bytes: 1_173_378, access: 'free', disk: PEER_MEDIA('photo-city-neon.jpg'),
    description: '1920×2349 · Dotonbori canal at night, neon signs bleeding into the water · Osaka, Japan · Wikimedia Commons, CC BY-SA 4.0.' },
  { id: 'photo-desert-dunes', filename: 'desert-dunes-dawn.jpg', mime_type: 'image/jpeg', size_bytes: 489_931, access: 'free', disk: PEER_MEDIA('photo-desert-dunes.jpg'),
    description: '1920×1281 · Low sun raking across rippled sand, Erg Chebbi · Morocco · Wikimedia Commons, CC BY-SA 4.0.' },
  { id: 'photo-forest-mist', filename: 'forest-mist-morning.jpg', mime_type: 'image/jpeg', size_bytes: 423_000, access: 'free', disk: PEER_MEDIA('photo-forest-mist.jpg'),
    description: '1920×1204 · Sunrise mist in the valleys above Schonach, Black Forest · Germany · Wikimedia Commons, Free Art License.' },
  { id: 'photo-ocean-cliff', filename: 'ocean-cliff-atlantic.jpg', mime_type: 'image/jpeg', size_bytes: 683_730, access: 'free', disk: PEER_MEDIA('photo-ocean-cliff.jpg'),
    description: '1920×1440 · Atlantic swell breaking under the Cliffs of Moher · Ireland · Wikimedia Commons, CC BY 2.0.' },
  { id: 'photo-northern-road', filename: 'northern-road-solitude.jpg', mime_type: 'image/jpeg', size_bytes: 643_159, access: 'free', disk: PEER_MEDIA('photo-northern-road.jpg'),
    description: '1920×1280 · Empty winter road toward a backlit peak · Iceland · Wikimedia Commons, CC0.' },
  { id: 'photo-autumn-valley', filename: 'autumn-valley-patchwork.jpg', mime_type: 'image/jpeg', size_bytes: 872_237, access: 'free', disk: PEER_MEDIA('photo-autumn-valley.jpg'),
    description: '1920×1205 · White steeple against peak fall foliage, Stowe · Vermont, USA · Wikimedia Commons, CC BY 2.0.' },
  { id: 'photo-harbor-dawn', filename: 'harbor-golden-hour.jpg', mime_type: 'image/jpeg', size_bytes: 655_311, access: 'free', disk: PEER_MEDIA('photo-harbor-dawn.jpg'),
    description: '1920×1080 · St Ives harbour wall at low tide, golden evening light · Cornwall, UK · Wikimedia Commons, CC BY-SA 4.0.' },
  { id: 'photo-alpine-ridge', filename: 'alpine-ridge-hiker.jpg', mime_type: 'image/jpeg', size_bytes: 642_889, access: 'free', disk: PEER_MEDIA('photo-alpine-ridge.jpg'),
    description: '1920×1280 · Hiker crossing a snowfield above Grindelwald, Bernese Oberland · Switzerland · Wikimedia Commons, CC BY 2.0.' },
  // Documents — free, real committed files
  { id: 'doc-whitepaper-notes', filename: 'bitcoin-whitepaper-notes.md', mime_type: 'text/markdown', size_bytes: 8_192, access: 'free', disk: PEER_CONTENT('documents', 'bitcoin-whitepaper-notes.md'),
    description: 'Markdown study notes on the Bitcoin whitepaper, section by section.' },
  { id: 'doc-sovereignty-manifesto', filename: 'sovereignty-manifesto.txt', mime_type: 'text/plain', size_bytes: 4_096, access: 'free', disk: PEER_CONTENT('documents', 'sovereignty-manifesto.txt'),
    description: 'Plain-text manifesto on self-hosted infrastructure.' },
  { id: 'doc-node-checklist', filename: 'node-setup-checklist.md', mime_type: 'text/markdown', size_bytes: 6_144, access: 'free', disk: PEER_CONTENT('documents', 'node-setup-checklist.md'),
    description: 'Step-by-step hardening checklist for a fresh Archipelago node.' },
  { id: 'doc-lightning-csv', filename: 'lightning-channels.csv', mime_type: 'text/csv', size_bytes: 2_048, access: 'free', disk: PEER_CONTENT('documents', 'lightning-channels.csv'),
    description: 'CSV export — public channel set with capacities and fee policies.' },
  // Software
  { id: 'sw-archy-iso', filename: 'archipelago-1.7.99-amd64.iso', mime_type: 'application/x-iso9660-image', size_bytes: 2_684_354_560, access: 'free',
    description: 'Archipelago Node OS installer ISO · amd64 · SHA256 in release notes.' },
]

/** Deterministic per-peer slice of the library. Each item lives on exactly ONE
 *  peer (slot = library index % 12, matched to the peer's position in this
 *  session's federation list), except a small POPULAR set that also appears on
 *  1-2 more nodes — a little duplication is realistic, a catalog where every
 *  file exists on a third of the network is not. */
const POPULAR_EXTRA_SLOTS = {
  'song-webfive': [3],        // the showcase paid Wavlake track — easy to find
  'film-block-height': [5],
  'photo-aurora-fjord': [8],
}
function peerCatalogFor(onion) {
  const nodes = sessionFederationNodes()
  let slot = nodes.findIndex(n => n.onion === onion)
  if (slot < 0) {
    // Unknown onion (e.g. a stale deep-link) — stable hash fallback.
    let seed = 0
    for (const c of String(onion)) seed = (seed * 31 + c.charCodeAt(0)) >>> 0
    slot = seed % 12
  }
  return PEER_LIBRARY
    .map((item, i) => ({ item, i }))
    .filter(({ item, i }) => i % 12 === slot || (POPULAR_EXTRA_SLOTS[item.id] || []).includes(slot))
    .map(({ item }) => ({
      id: item.id,
      filename: item.filename,
      mime_type: item.mime_type,
      size_bytes: item.size_bytes,
      description: item.description,
      access: item.access,
    }))
}

/** Purchase history ("Paid Files" tab + Owned badges). Lazily seeded per
 *  session with a few plausible past purchases whose onions/content ids match
 *  THIS session's federation nodes and their catalog slices (onions are random
 *  per session, so this must be computed, never hardcoded). Every successful
 *  purchase path appends here so Owned state survives loadOwned() refreshes. */
function sessionOwnedContent() {
  if (!mockState.ownedContent) {
    const nodes = sessionFederationNodes()
    const seeds = []
    const wanted = [
      // At least one audio item with real disk bytes: the Paid Files tab click
      // must actually play music in the bottom bar.
      { id: 'song-builders', daysAgo: 4 },
      { id: 'film-the-signal', daysAgo: 11 },
      { id: 'book-cypherpunk-essays', daysAgo: 23 },
    ]
    for (const w of wanted) {
      for (const n of nodes) {
        const item = peerCatalogFor(n.onion).find(it => it.id === w.id)
        if (!item) continue
        seeds.push({
          onion: n.onion,
          content_id: item.id,
          filename: item.filename,
          mime_type: item.mime_type,
          size_bytes: item.size_bytes,
          paid_sats: (typeof item.access === 'object' && item.access.paid?.price_sats) || 0,
          purchased_at: new Date(Date.now() - w.daysAgo * 86400000).toISOString(),
        })
        break
      }
    }
    mockState.ownedContent = seeds
  }
  return mockState.ownedContent
}

// 12 trusted/federated nodes for the demo Federation view. Onions are random,
// so the list is memoised per session — peer catalogs, owned-content records
// and the Federation view must all agree on the same onions for a visitor.
function sessionFederationNodes() {
  if (!mockState.federationNodes) mockState.federationNodes = demoFederationNodes()
  return mockState.federationNodes
}
function demoFederationNodes() {
  const names = [
    ['archy-198', 'trusted', ['bitcoin-knots', 'lnd', 'mempool', 'electrs']],
    ['arch-tailscale-1', 'trusted', ['bitcoin-knots', 'lnd', 'nextcloud']],
    ['bunker-alpha', 'trusted', ['bitcoin-knots', 'vaultwarden']],
    ['seaside-node', 'trusted', ['bitcoin-knots', 'lnd', 'fedimint']],
    ['highland-relay', 'trusted', ['bitcoin-knots', 'electrs', 'mempool']],
    ['orchard-12', 'trusted', ['bitcoin-knots', 'immich']],
    ['nimbus-vps', 'trusted', ['bitcoin-knots', 'lnd', 'thunderhub']],
    ['rivertown', 'trusted', ['bitcoin-knots', 'jellyfin']],
    ['granite-pi', 'trusted', ['bitcoin-knots', 'lnd']],
    ['saltmarsh', 'observer', ['bitcoin-knots']],
    ['dustbowl-node', 'observer', ['bitcoin-knots', 'electrs']],
    ['frontier-7', 'observer', ['bitcoin-knots', 'mempool']],
  ]
  return names.map(([name, trust, apps], i) => {
    const seenAgo = 60000 + i * 95000
    return {
      did: `did:key:z6Mk${randomHex(20)}`,
      pubkey: randomHex(32),
      onion: `${name.replace(/[^a-z0-9]/g, '')}${randomHex(8)}.onion`,
      trust_level: trust,
      added_at: new Date(Date.now() - (i + 2) * 86400000).toISOString(),
      name,
      last_seen: new Date(Date.now() - seenAgo).toISOString(),
      last_state: {
        timestamp: new Date(Date.now() - seenAgo).toISOString(),
        apps: apps.map(id => ({ id, status: 'running', version: '1.0' })),
        cpu_usage_percent: Math.round((6 + (i * 7) % 55) * 10) / 10,
        mem_used_bytes: (2 + (i % 6)) * 1_000_000_000,
        mem_total_bytes: (i % 2 ? 32 : 16) * 1_000_000_000,
        disk_used_bytes: (400 + i * 90) * 1_000_000_000,
        disk_total_bytes: (i % 2 ? 2000 : 1000) * 1_000_000_000,
        uptime_secs: 86400 * (i + 1),
        tor_active: trust === 'trusted',
      },
    }
  })
}

// Serve each real registry UI's WHOLE directory at its /app/<id>/ path, so the
// shell's bundled assets (qrcode.js, css, bg images, icons) resolve via their
// relative paths. The data endpoints each shell polls are mocked separately.
const DOCKER_UI = path.join(__dirname, '..', 'docker')
app.use('/app/justworks', express.static(path.join(DOCKER_UI, 'justworks/public')))
app.get('/app/justworks/setup', (_req, res) => res.sendFile(path.join(DOCKER_UI, 'justworks/public/index.html')))
for (const [prefixes, dir] of [
  [['/app/bitcoin-core', '/app/bitcoin-knots', '/app/bitcoin-ui'], 'bitcoin-ui'],
  [['/app/electrumx', '/app/electrs', '/app/archy-electrs-ui'], 'electrs-ui'],
  // lnd deliberately NOT here: the real lnd-ui shell reads poorly in the demo
  // iframe, so /app/lnd/ gets a DEMO_APP_PAGES placeholder dashboard instead.
  [['/app/fedimint', '/app/fedimintd'], 'fedimint-ui'],
]) {
  for (const p of prefixes) app.use(p, express.static(path.join(DOCKER_UI, dir)))
}
// Shells that reference /app/<id>/assets/... resolve to the frontend's shared assets.
// Demo placeholder for mempool: the real explorer needs a synced chain +
// electrs, which the public demo doesn't run. Show a branded "not available
// in demo" page instead of a 502. Self-contained (inline styles/logo) so it
// renders identically from the mock origin in dev and in the demo stack.
app.get(/^\/app\/mempool(\/.*)?$/, (_req, res) => {
  res.type('html').send(`<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Mempool</title></head>
<body style="margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#11131f;font-family:-apple-system,'Segoe UI',Roboto,sans-serif;">
  <div style="text-align:center;padding:2rem;">
    <div style="display:flex;gap:6px;justify-content:center;margin-bottom:1.25rem;">
      <div style="width:34px;height:34px;border-radius:7px;background:#9339f4;"></div>
      <div style="width:34px;height:34px;border-radius:7px;background:#105fb0;"></div>
      <div style="width:34px;height:34px;border-radius:7px;background:#00a0dc;"></div>
      <div style="width:34px;height:34px;border-radius:7px;background:#00c896;"></div>
    </div>
    <div style="font-size:1.6rem;font-weight:700;color:#fff;letter-spacing:.02em;">mempool</div>
    <p style="color:rgba(255,255,255,.55);font-size:.95rem;margin-top:.75rem;">Not available in the demo</p>
    <p style="color:rgba(255,255,255,.35);font-size:.8rem;max-width:340px;margin:0.5rem auto 0;">
      The block explorer runs against your node's own synced chain — install Archipelago to explore mempool.space-style data privately.
    </p>
  </div>
</body></html>`)
})

app.get(/^\/app\/[^/]+\/assets\/(.*)$/, (req, res) => res.redirect(302, '/assets/' + req.params[0]))

// Dummy status for both the electrs-ui shell and the in-app ElectrumX sync screen.
app.get('/electrs-status', (_req, res) => {
  res.json({
    status: 'ready',
    synced: true,
    stale: false,
    error: null,
    bitcoin_height: 902418,
    network_height: 902418,
    indexed_height: 902418,
    progress_pct: 100,
    index_size: '58.2 GB',
    tor_onion: 'electrumx' + randomHex(20) + '.onion',
  })
})

// ── Bitcoin Core + Knots: per-implementation status (shell served statically) ─
function bitcoinStatusPayload(impl) {
  const net = mockBitcoinNetworkInfo()
  net.subversion = impl === 'knots' ? '/Satoshi:28.1.0/Knots:20250305/' : '/Satoshi:28.4.0/'
  return {
    ok: true, stale: false, updated_at_ms: Date.now(), error: null,
    blockchain_info: mockBitcoinBlockchainInfo(),
    network_info: net,
    index_info: {
      txindex: { synced: true, best_block_height: 902418 },
      blockfilterindex: { synced: true, best_block_height: 902418 },
    },
    zmq_notifications: [
      { type: 'pubhashblock', address: 'tcp://127.0.0.1:28332', hwm: 1000 },
      { type: 'pubrawtx', address: 'tcp://127.0.0.1:28333', hwm: 1000 },
    ],
  }
}
app.get('/app/bitcoin-core/bitcoin-status', (_req, res) => res.json(bitcoinStatusPayload('core')))
app.get('/app/bitcoin-knots/bitcoin-status', (_req, res) => res.json(bitcoinStatusPayload('knots')))

// ── LND: the endpoints the lnd-ui shell polls (shell served statically) ──────
function lndGetinfo() {
  return {
    alias: 'archipelago-lnd', identity_pubkey: '02' + randomHex(32),
    num_active_channels: 4, num_inactive_channels: 0, num_pending_channels: 0, num_peers: 11,
    block_height: 902418, block_hash: randomHex(32),
    synced_to_chain: true, synced_to_graph: true, version: '0.18.3-beta',
    chains: [{ chain: 'bitcoin', network: 'signet' }],
  }
}
function lndChannels() {
  const peers = [['ACINQ', 5_000_000, 2_450_000], ['Wallet of Satoshi', 2_000_000, 1_200_000],
    ['Voltage', 10_000_000, 4_500_000], ['Kraken', 3_000_000, 1_800_000]]
  return {
    channels: peers.map(([alias, cap, local]) => ({
      active: true, remote_pubkey: '02' + randomHex(32), peer_alias: alias,
      capacity: String(cap), local_balance: String(local), remote_balance: String(cap - local),
      total_satoshis_sent: '12840', total_satoshis_received: '9120', private: false,
    })),
  }
}
app.get('/proxy/lnd/v1/getinfo', (_req, res) => res.json(lndGetinfo()))
app.get('/proxy/lnd/v1/channels', (_req, res) => res.json(lndChannels()))
// Balances polled by the lnd-ui shell's stat cards (LND REST shapes).
app.get('/proxy/lnd/v1/balance/blockchain', (_req, res) => res.json({
  total_balance: '2450000', confirmed_balance: '2350000', unconfirmed_balance: '100000',
}))
app.get('/proxy/lnd/v1/balance/channels', (_req, res) => res.json({
  balance: '8250000',
  local_balance: { sat: '8250000', msat: '8250000000' },
  remote_balance: { sat: '11750000', msat: '11750000000' },
  pending_open_local_balance: { sat: '500000', msat: '500000000' },
}))
// The shell only renders the "Connect Your Wallet" QR + details when
// cert_base64url/macaroon_base64url and the ports are present.
app.get('/lnd-connect-info', (_req, res) => res.json({
  getinfo: lndGetinfo(), channelCount: 4,
  cert_base64url: Buffer.from('demo-tls-cert-' + randomHex(24)).toString('base64url'),
  macaroon_base64url: Buffer.from('demo-admin-macaroon-' + randomHex(32)).toString('base64url'),
  grpc_port: 10009, rest_port: 8080,
  grpcReachable: true, restReachable: true,
  tor_onion: 'lnd' + randomHex(16) + '.onion', error: null,
}))
// App catalog (the discover page tries this first, then /catalog.json).
app.get('/api/app-catalog', async (_req, res) => {
  try {
    const json = await fs.readFile(path.join(__dirname, 'public', 'catalog.json'), 'utf8')
    res.type('application/json').send(json)
  } catch { res.status(404).json({ error: 'not found' }) }
})
// Mesh attachment upload (Tor/content-ref path) — mirrors the real node's
// blob endpoint: raw bytes in, {cid} out. Bytes stay in the visitor's
// per-session store; mesh.fetch-content serves them back as a data: URL.
app.post('/api/blob', express.raw({ type: () => true, limit: '25mb' }), (req, res) => {
  const buf = Buffer.isBuffer(req.body) ? req.body : Buffer.alloc(0)
  const cid = 'demo-blob-' + crypto.createHash('sha256').update(buf).digest('hex').slice(0, 16)
  currentStore().mesh.blobs[cid] = {
    mime: req.get('X-Blob-Mime') || 'application/octet-stream',
    filename: req.get('X-Blob-Filename') || null,
    thumb_b64: req.get('X-Blob-Thumb') || null,
    b64: buf.toString('base64'),
  }
  res.json({ cid })
})
// Free peer-content streaming (Range-capable). The frontend streams free
// audio/video and the free-image lightbox from this URL; on a real node nginx
// proxies /api/* to the daemon. content_id resolves strictly against
// PEER_LIBRARY (whitelist — never a filesystem path built from the request),
// and paid items 403 so full paid bytes are only reachable via a purchase.
app.get('/api/peer-content/:onion/:content_id', (req, res) => {
  const entry = PEER_LIBRARY.find(it => it.id === req.params.content_id)
  if (!entry || !entry.disk) return res.status(404).json({ error: 'Content not found on this peer' })
  if (typeof entry.access === 'object') return res.status(403).json({ error: 'This file requires purchase' })
  res.type(entry.mime_type)
  // Express sendFile handles Range requests (the frontend probes bytes=0-0).
  res.sendFile(entry.disk)
})
app.get('/api/container/logs', (_req, res) => res.json({
  logs: [
    '[INF] LND: Version 0.18.3-beta commit=v0.18.3-beta',
    '[INF] CHDB: Inserting 4 channel(s) into database',
    '[INF] LND: Channel(s) active; synced_to_chain=true',
    '[INF] PEER: Connected to 11 peers',
    '[INF] RPCS: gRPC proxy started at 0.0.0.0:8080',
  ].join('\n'),
}))

app.get('/app/bitcoin-ui/bitcoin-status', (_req, res) => res.json(bitcoinStatusPayload('knots')))

app.post(['/app/bitcoin-ui/bitcoin-rpc/', '/app/bitcoin-core/bitcoin-rpc/', '/app/bitcoin-knots/bitcoin-rpc/'], (req, res) => {
  const { id = 'bitcoin-ui', method } = req.body || {}
  const results = {
    getblockchaininfo: mockBitcoinBlockchainInfo(),
    getnetworkinfo: mockBitcoinNetworkInfo(),
    getindexinfo: {
      txindex: { synced: true, best_block_height: 902418 },
      blockfilterindex: { synced: true, best_block_height: 902418 },
    },
    getzmqnotifications: [
      { type: 'pubhashblock', address: 'tcp://127.0.0.1:28332', hwm: 1000 },
      { type: 'pubrawtx', address: 'tcp://127.0.0.1:28333', hwm: 1000 },
    ],
    getmempoolinfo: { loaded: true, size: 18452, bytes: 62400000, usage: 143000000, maxmempool: 300000000 },
    getpeerinfo: Array.from({ length: 18 }, (_, index) => ({ id: index, addr: `198.51.100.${index + 10}:8333`, inbound: index % 3 === 0 })),
  }
  if (!Object.prototype.hasOwnProperty.call(results, method)) {
    return res.json({ id, result: null, error: { code: -32601, message: `Method not found: ${method}` } })
  }
  res.json({ id, result: results[method], error: null })
})

app.post(['/app/bitcoin-ui/rpc/v1', '/app/bitcoin-core/rpc/v1', '/app/bitcoin-knots/rpc/v1'], (req, res) => {
  const { method, params = {} } = req.body || {}
  const now = new Date().toISOString()
  switch (method) {
    case 'bitcoin.relay-status':
      return res.json({ result: bitcoinRelayStatusPayload(), error: null })
    case 'bitcoin.relay-update-settings':
      bitcoinRelayMockState.settings = {
        ...bitcoinRelayMockState.settings,
        ...params,
      }
      return res.json({ result: bitcoinRelayStatusPayload(), error: null })
    case 'bitcoin.relay-request-peer': {
      const peer = bitcoinRelayMockState.trusted_nodes.find(p => p.pubkey === params.peer_pubkey)
      if (!peer) return res.status(400).json({ error: { message: 'Peer is not in trusted nodes' } })
      const request = {
        id: `relay-demo-${Date.now()}`,
        direction: 'outbound',
        status: 'pending',
        peer_pubkey: peer.pubkey,
        peer_onion: peer.onion,
        peer_name: peer.name,
        message: params.message || '',
        created_at: now,
        updated_at: now,
      }
      bitcoinRelayMockState.requests.push(request)
      return res.json({ result: { ok: true, request_id: request.id }, error: null })
    }
    case 'bitcoin.relay-approve-request':
    case 'bitcoin.relay-reject-request': {
      const requestId = params.id || params.request_id
      const request = bitcoinRelayMockState.requests.find(r => r.id === requestId)
      if (!request) return res.status(404).json({ error: { message: `Request not found: ${requestId}` } })
      request.status = method.endsWith('approve-request') ? 'approved' : 'rejected'
      request.updated_at = now
      if (request.status === 'approved') {
        request.approved_endpoint = bitcoinRelayMockState.settings.https_endpoint || bitcoinRelayMockState.settings.tor_endpoint || bitcoinRelayMockState.settings.http_endpoint
        request.credential_secret_path = '/var/lib/archipelago/secrets/bitcoin-relay-peer-demo.env'
      }
      return res.json({ result: { ok: true, request_id: request.id }, error: null })
    }
    case 'bitcoin.relay-create-tor-service':
      bitcoinRelayMockState.settings.allow_tor = true
      bitcoinRelayMockState.settings.tor_endpoint = 'http://btc-relay-demoabcdefghijklmnop.onion/'
      return res.json({
        result: {
          created: true,
          name: 'bitcoin-rpc',
          onion_address: 'btc-relay-demoabcdefghijklmnop.onion',
        },
        error: null,
      })
    default:
      return res.status(404).json({ error: { message: `Unknown mock Bitcoin UI RPC method: ${method}` } })
  }
})

// RPC endpoint
app.post('/rpc/v1', (req, res) => {
  const { method, params } = req.body
  console.log(`[RPC] ${method}`)

  // Boot mode: return 502 during simulated startup delay
  if (DEV_MODE === 'boot') {
    // Reset boot timer when browser does a fresh page load (server.echo with 'boot' message)
    if (method === 'server.echo' && params?.message === 'boot-reset') {
      BOOT_START_TIME = Date.now()
      console.log(`[Boot] Timer RESET — simulating ${BOOT_DELAY_MS / 1000}s startup`)
      return res.status(502).json({ error: 'Server starting up (reset)' })
    }
    const elapsed = Date.now() - BOOT_START_TIME
    if (elapsed < BOOT_DELAY_MS) {
      const secs = Math.round(elapsed / 1000)
      const total = Math.round(BOOT_DELAY_MS / 1000)
      console.log(`[Boot] Server starting... ${secs}s / ${total}s`)
      return res.status(502).json({ error: 'Server starting up' })
    }
    if (elapsed < BOOT_DELAY_MS + 2000) {
      console.log(`[Boot] Server is now READY (took ${Math.round(elapsed / 1000)}s)`)
    }
  }

  try {
    switch (method) {
      // Authentication endpoints
      case 'auth.setup': {
        const { password } = params
        
        if (!password || password.length < 8) {
          return res.json({
            error: {
              code: -32602,
              message: 'Password must be at least 8 characters',
            },
          })
        }
        
        // Set up user
        userState.setupComplete = true
        userState.passwordHash = password // In real app, bcrypt hash
        
        console.log(`[Auth] User setup completed`)
        
        return res.json({ result: { success: true } })
      }

      case 'auth.isSetup': {
        return res.json({ result: userState.setupComplete })
      }

      case 'auth.onboardingComplete': {
        userState.onboardingComplete = true
        console.log(`[Auth] Onboarding completed`)
        return res.json({ result: true })
      }

      case 'auth.isOnboardingComplete': {
        return res.json({ result: userState.onboardingComplete })
      }

      case 'auth.resetOnboarding': {
        userState.onboardingComplete = false
        console.log('[Auth] Onboarding reset')
        return res.json({ result: true })
      }

      case 'app.filebrowser-token': {
        // The Cloud/Files UI exchanges this for a filebrowser auth cookie.
        // The mock filebrowser endpoints don't validate it, so any token works.
        return res.json({ result: { token: `demo-fb-${Date.now().toString(36)}` } })
      }
      case 'node.did': {
        const mockDid = 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH'
        const mockPubkey = 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456'
        return res.json({ result: { did: mockDid, pubkey: mockPubkey } })
      }
      case 'node.nostr-publish': {
        return res.json({ result: { event_id: 'mock-event-id', success: 2, failed: 0 } })
      }
      case 'node.nostr-pubkey': {
        // Exact shape of the real handler (node.rs handle_node_nostr_pubkey):
        // the node's dedicated discovery key — hex + genuine bech32 npub
        // (deliberately NOT one of the personal identity keys; discovery
        // presence events are always signed with this separate node key).
        return res.json({ result: {
          nostr_pubkey: 'c9f0a2e84b71d3568e0f4a6b2c8d1e97a3b5c7d9e1f2a4b6c8d0e2f4a6b8c0d2',
          nostr_npub: 'npub1e8c296ztw8f4drs0ff4jerg7j73mt37eu8e2fdkg6r30ff4ccrfq8j5vuy',
        } })
      }

      case 'node.signChallenge': {
        const { challenge } = params || {}
        const mockSig = Buffer.from(`mock-sig-${challenge || 'challenge'}`).toString('hex')
        return res.json({ result: { signature: mockSig } })
      }

      case 'identity.verify': {
        return res.json({ result: { valid: true } })
      }

      // BIP-39 seed management (mock for dev mode)
      case 'seed.generate': {
        const mockWords = [
          'abandon', 'ability', 'able', 'about', 'above', 'absent',
          'absorb', 'abstract', 'absurd', 'abuse', 'access', 'accident',
          'account', 'accuse', 'achieve', 'acid', 'acoustic', 'acquire',
          'across', 'act', 'action', 'actor', 'actress', 'actual'
        ]
        return res.json({ result: { words: mockWords } })
      }
      case 'seed.verify': {
        const mockDid = 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH'
        return res.json({ result: { verified: true, did: mockDid, nostr_npub: 'npub1mock...' } })
      }
      case 'seed.restore': {
        const mockDid = 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH'
        return res.json({ result: { did: mockDid, nostr_npub: 'npub1mock...', restored: true } })
      }
      case 'seed.save-encrypted': {
        return res.json({ result: { saved: true } })
      }
      case 'seed.status': {
        return res.json({ result: { has_seed: true, is_legacy: false, identity_count: 1, next_index: 1 } })
      }

      case 'node.createBackup': {
        const { passphrase } = params || {}
        if (!passphrase) {
          return res.json({ error: { code: -32602, message: 'Missing passphrase' } })
        }
        const mockDid = 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH'
        const mockPubkey = 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456'
        return res.json({
          result: {
            version: 1,
            did: mockDid,
            pubkey: mockPubkey,
            kid: `${mockDid}#key-1`,
            encrypted: true,
            blob: Buffer.from(`mock-encrypted-backup-${passphrase}`).toString('base64'),
            timestamp: new Date().toISOString(),
          },
        })
      }

      // The dashboard uses this read-only RPC to validate its session on reload.
      // Keep demo deep links (including setup guides) compatible with that probe.
      case 'system.get-hostname':
        return res.json({ result: { hostname: 'archipelago-demo', mdns_hostname: 'archipelago-demo.local', lan_ip: '127.0.0.1' } })

      case 'auth.login': {
        const { password } = params
        
        if (!userState.setupComplete) {
          return res.json({
            error: {
              code: -32603,
              message: 'User not set up. Please complete setup first.',
            },
          })
        }
        
        // Simple password check (in real app, use bcrypt)
        if (password !== userState.passwordHash && password !== MOCK_PASSWORD) {
          return res.json({
            error: {
              code: -32603,
              message: 'Password Incorrect',
            },
          })
        }

        const sessionId = `session-${Date.now()}`
        sessions.set(sessionId, {
          createdAt: new Date(),
        })

        res.cookie('session', sessionId, {
          httpOnly: true,
          maxAge: 24 * 60 * 60 * 1000,
        })

        return res.json({ result: null })
      }

      case 'auth.logout': {
        const sessionId = req.cookies?.session
        if (sessionId) {
          sessions.delete(sessionId)
        }
        res.clearCookie('session')
        return res.json({ result: null })
      }

      case 'server.set-name': {
        const name = (params?.name || '').trim()
        if (!name || name.length > 64) {
          return res.json({ error: { code: -1, message: 'Name must be 1-64 characters' } })
        }
        mockData['server-info'].name = name
        broadcastUpdate()
        return res.json({ result: { name } })
      }

      case 'server.echo': {
        return res.json({ result: { message: params?.message || 'Hello from Archipelago!' } })
      }

      case 'server.get-state': {
        // Full state snapshot — the sync store polls this every 30s and after
        // every WebSocket reconnect. Without it the UI can never resync (and
        // logged a "Method not found" error each time).
        return res.json({ result: { data: mockData } })
      }

      case 'server.time': {
        return res.json({
          result: {
            now: new Date().toISOString(),
            uptime: process.uptime(),
          },
        })
      }

      case 'server.metrics': {
        // Slightly randomize so the dashboard feels alive
        return res.json({
          result: {
            cpu: +(12 + Math.random() * 18).toFixed(1),
            memory: +(58 + Math.random() * 8).toFixed(1),
            disk: +(34 + Math.random() * 3).toFixed(1),
          },
        })
      }

      case 'marketplace.get': {
        const mockApps = [
          {
            id: 'bitcoin',
            title: 'Bitcoin Core',
            description: 'Full Bitcoin node — validate transactions, enforce consensus rules, and support the network. The foundation of sovereignty.',
            version: '27.1',
            icon: '/assets/img/app-icons/bitcoin-core.png',
            author: 'Bitcoin Core',
            license: 'MIT',
            category: 'Bitcoin',
          },
          {
            id: 'lnd',
            title: 'LND',
            description: 'Lightning Network Daemon — instant, low-fee Bitcoin payments. Open channels, route payments, earn routing fees.',
            version: '0.18.3',
            icon: '/assets/img/app-icons/lnd.png',
            author: 'Lightning Labs',
            license: 'MIT',
            category: 'Bitcoin',
          },
          {
            id: 'electrs',
            title: 'Electrs',
            description: 'Electrum Server in Rust — index the blockchain for fast wallet lookups. Connect your hardware wallets privately.',
            version: '0.10.6',
            icon: '/assets/img/app-icons/electrumx.webp',
            author: 'Roman Zeyde',
            license: 'MIT',
            category: 'Bitcoin',
          },
          {
            id: 'mempool',
            title: 'Mempool',
            description: 'Bitcoin blockchain explorer and mempool visualizer. Monitor transactions, fees, and network activity in real time.',
            version: '3.0.0',
            icon: '/assets/img/app-icons/mempool.webp',
            author: 'Mempool Space',
            license: 'AGPL-3.0',
            category: 'Bitcoin',
          },
          {
            id: 'btcpay',
            title: 'BTCPay Server',
            description: 'Self-hosted Bitcoin payment processor. Accept Bitcoin payments in your store — no fees, no middlemen, no KYC.',
            version: '2.0.4',
            icon: '/assets/img/app-icons/btcpay-server.png',
            author: 'BTCPay Server',
            license: 'MIT',
            category: 'Commerce',
          },
          {
            id: 'fedimint',
            title: 'Fedimint',
            description: 'Federated Chaumian e-cash mint — community custody, private payments, and Lightning gateways for your tribe.',
            version: '0.10.0',
            icon: '/assets/img/app-icons/fedimint.png',
            author: 'Fedimint',
            license: 'MIT',
            category: 'Bitcoin',
          },
          {
            id: 'thunderhub',
            title: 'ThunderHub',
            description: 'Lightning node management UI — manage channels, send and receive payments, view routing fees, and monitor your node health.',
            version: '0.13.31',
            icon: '/assets/img/app-icons/thunderhub.svg',
            author: 'Anthony Potdevin',
            license: 'MIT',
            category: 'Bitcoin',
          },
          {
            id: 'vaultwarden',
            title: 'Vaultwarden',
            description: 'Self-hosted password manager compatible with Bitwarden clients. Keep your credentials under your own roof.',
            version: '1.32.5',
            icon: '/assets/img/app-icons/vaultwarden.webp',
            author: 'Vaultwarden',
            license: 'AGPL-3.0',
            category: 'Privacy',
          },
          {
            id: 'nextcloud',
            title: 'Nextcloud',
            description: 'Your personal cloud — files, calendar, contacts, and collaboration. Replace Google Drive and Dropbox entirely.',
            version: '29.0.0',
            icon: '/assets/img/app-icons/nextcloud.webp',
            author: 'Nextcloud GmbH',
            license: 'AGPL-3.0',
            category: 'Cloud',
          },
          {
            id: 'nostr-relay',
            title: 'Nostr Relay',
            description: 'Run your own Nostr relay — sovereign social networking. Publish notes, follow friends, no censorship possible.',
            version: '0.34.0',
            icon: '/assets/img/app-icons/nostrudel.svg',
            author: 'nostr-rs-relay',
            license: 'MIT',
            category: 'Social',
          },
          {
            id: 'home-assistant',
            title: 'Home Assistant',
            description: 'Open-source home automation — control lights, locks, cameras, and sensors. Smart home without the cloud dependency.',
            version: '2024.12.0',
            icon: '/assets/img/app-icons/homeassistant.png',
            author: 'Home Assistant',
            license: 'Apache-2.0',
            category: 'IoT',
          },
          {
            id: 'syncthing',
            title: 'Syncthing',
            description: 'Continuous peer-to-peer file synchronization. Sync folders across devices without any cloud service.',
            version: '1.28.1',
            icon: '/assets/img/app-icons/archipelago-a.svg',
            author: 'Syncthing Foundation',
            license: 'MPL-2.0',
            category: 'Cloud',
          },
          {
            id: 'tor',
            title: 'Tor',
            description: 'Anonymous communication — route traffic through the Tor network. Access your node from anywhere, privately.',
            version: '0.4.8.13',
            icon: '/assets/img/app-icons/tor.svg',
            author: 'Tor Project',
            license: 'BSD-3',
            category: 'Privacy',
          },
        ]

        return res.json({ result: mockApps })
      }

      case 'server.update':
      case 'server.restart':
      case 'server.shutdown': {
        return res.json({ result: 'ok' })
      }

      case 'package.install': {
        const { id, url, dockerImage, version } = params

        installPackage(id, url, { dockerImage, version }).catch(err => {
          console.error(`[RPC] Installation failed:`, err.message)
        })

        return res.json({ result: `job-${Date.now()}` })
      }

      case 'package.uninstall': {
        const { id } = params
        
        uninstallPackage(id).catch(err => {
          console.error(`[RPC] Uninstall failed:`, err.message)
        })
        
        return res.json({ result: 'ok' })
      }

      case 'package.start':
      case 'package.stop':
      case 'package.restart': {
        return res.json({ result: 'ok' })
      }

      case 'auth.totp.status': {
        return res.json({ result: { enabled: false } })
      }

      case 'auth.totp.setup.begin': {
        return res.json({
          result: {
            qr_svg: '<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200"><rect width="200" height="200" fill="#fff"/><text x="100" y="100" text-anchor="middle" font-size="12" fill="#333">Mock QR Code</text></svg>',
            secret_base32: 'JBSWY3DPEHPK3PXP',
            pending_token: 'mock-pending-token',
          },
        })
      }

      case 'auth.totp.setup.confirm': {
        return res.json({
          result: {
            enabled: true,
            backup_codes: ['ABCD-EFGH', 'JKLM-NPQR', 'STUV-WXYZ', '2345-6789', 'ABCD-2345', 'EFGH-6789', 'JKLM-STUV', 'NPQR-WXYZ'],
          },
        })
      }

      case 'auth.totp.disable': {
        return res.json({ result: { disabled: true } })
      }

      case 'auth.login.totp':
      case 'auth.login.backup': {
        return res.json({ result: { success: true } })
      }

      // =========================================================================
      // Identity & Onboarding
      // =========================================================================
      case 'identity.create': {
        const { name, purpose } = params || {}
        console.log(`[Identity] Created identity: "${name || 'Personal'}" (${purpose || 'personal'})`)
        return res.json({ result: { success: true, id: `identity-${Date.now()}` } })
      }

      case 'identity.register-name': {
        const { name } = params || {}
        console.log(`[Identity] Registered name: ${name}`)
        return res.json({ result: { success: true, id: `name-${Date.now()}` } })
      }

      case 'identity.remove-name': {
        return res.json({ result: { success: true } })
      }

      case 'identity.set-default': {
        return res.json({ result: { success: true } })
      }

      case 'identity.delete': {
        return res.json({ result: { success: true } })
      }

      case 'identity.issue-credential': {
        return res.json({ result: { success: true, credential_id: `cred-${Date.now()}` } })
      }

      case 'identity.revoke-credential': {
        return res.json({ result: { success: true } })
      }

      case 'identity.list': {
        return res.json({
          result: {
            identities: [
              {
                id: 'id-primary',
                name: 'Primary',
                purpose: 'Main node identity',
                pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
                did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH',
                created_at: '2026-01-10T08:00:00Z',
                is_default: true,
                nostr_pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
                nostr_npub: 'npub1mockprimaryidentitypubkeyvalue0000000000000000000000000abc',
                profile: {
                  display_name: 'Archipelago Node',
                  about: 'Self-sovereign Bitcoin node',
                  nip05: 'satoshi@archipelago.local',
                  lud16: 'satoshi@getalby.com',
                  picture: '/demo-avatars/satoshi.svg',
                },
              },
              {
                id: 'id-anon',
                name: 'Anonymous',
                purpose: 'Privacy-focused browsing',
                pubkey: 'f6e5d4c3b2a19876543210fedcba9876543210fedcba9876543210fedcba98',
                did: 'did:key:z6MkvWkza1fMBWhKnYE3CgMgxHLKbN8NmbFRqHcECF4oGrwx',
                created_at: '2026-02-14T12:30:00Z',
                is_default: false,
                nostr_pubkey: 'f6e5d4c3b2a19876543210fedcba9876543210fedcba9876543210fedcba98',
                nostr_npub: 'npub1mockanonidentitypubkeyvalue000000000000000000000000000xyz',
                profile: { display_name: 'Anon', picture: '/demo-avatars/anon.svg' },
              },
              {
                id: 'id-merchant',
                name: 'Merchant',
                purpose: 'BTCPay & commerce',
                pubkey: '1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef',
                did: 'did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WNhSwaxN21V',
                created_at: '2026-03-01T16:00:00Z',
                is_default: false,
                nostr_pubkey: '1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef',
                nostr_npub: 'npub1mockmerchantidentitypubkeyvalue000000000000000000000000def',
                profile: { display_name: 'My Shop', website: 'https://myshop.onion', picture: '/demo-avatars/business.svg' },
              },
            ],
          },
        })
      }

      case 'identity.get': {
        const id = params?.id || 'id-primary'
        return res.json({
          result: {
            id,
            name: 'Primary',
            purpose: 'Main node identity',
            pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
            did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH',
            created_at: '2026-01-10T08:00:00Z',
            is_default: true,
            nostr_pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
            nostr_npub: 'npub1mockprimaryidentitypubkeyvalue0000000000000000000000000abc',
            profile: {
              display_name: 'Archipelago Node',
              about: 'Self-sovereign Bitcoin node',
              picture: '/demo-avatars/satoshi.svg',
            },
          },
        })
      }

      case 'identity.export-keys': {
        return res.json({
          result: {
            ed25519_secret_hex: 'deadbeef'.repeat(8),
            nostr_secret_hex: 'cafebabe'.repeat(8),
            nostr_nsec: 'nsec1mockexportedkeyvalue00000000000000000000000000000000000abc',
          },
        })
      }

      case 'identity.update-profile': {
        return res.json({ result: { success: true } })
      }

      case 'identity.publish-profile': {
        return res.json({ result: { event_id: `evt-${Date.now().toString(36)}` } })
      }

      case 'identity.list': {
        return res.json({
          result: {
            identities: [
              {
                id: 'id-primary',
                name: 'Primary',
                purpose: 'personal',
                pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
                did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH',
                created_at: '2026-01-10T08:00:00Z',
                is_default: true,
                nostr_pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
                nostr_npub: 'npub1598eg0y7m08htfzjfmzv6zjvf5u7p00dr9w0yfamaxqhkwlryckq5dh9ee',
                profile: {
                  display_name: 'Satoshi',
                  about: 'Running a sovereign Bitcoin node',
                  nip05: 'satoshi@archipelago.local',
                  lud16: 'satoshi@getalby.com',
                  picture: '/demo-avatars/satoshi.svg',
                },
              },
              {
                id: 'id-business',
                name: 'Business',
                purpose: 'business',
                pubkey: 'f6e5d4c3b2a10987654321fedcba0987654321fedcba0987654321fedcba0987',
                did: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9',
                created_at: '2026-02-05T12:30:00Z',
                is_default: false,
                nostr_pubkey: 'f6e5d4c3b2a10987654321fedcba0987654321fedcba0987654321fedcba0987',
                nostr_npub: 'npub17m9mx9kk2ry8p3xfkq0070fjlph9r9l0dj0y8fn0zrlj80ekjph7jxmxfg',
                profile: {
                  display_name: 'Archy Consulting',
                  about: 'Bitcoin infrastructure services',
                  picture: '/demo-avatars/business.svg',
                },
              },
              {
                id: 'id-anon',
                name: 'Anonymous',
                purpose: 'anonymous',
                pubkey: 'deadbeef00112233445566778899aabbccddeeff00112233445566778899aabb',
                did: 'did:key:z6MknGc3ocHs3zdPiJbnaaqDi5hjrZo4HzmQnwzaxWhAbWAs',
                created_at: '2026-03-01T18:00:00Z',
                is_default: false,
                profile: { picture: '/demo-avatars/anon.svg' },
              },
            ],
          },
        })
      }

      case 'identity.get': {
        const id = params?.id || 'id-primary'
        const identities = {
          'id-primary': {
            id: 'id-primary', name: 'Primary', purpose: 'personal',
            pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
            did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH',
            nostr_pubkey: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
            nostr_npub: 'npub1598eg0y7m08htfzjfmzv6zjvf5u7p00dr9w0yfamaxqhkwlryckq5dh9ee',
            is_default: true, created_at: '2026-01-10T08:00:00Z',
            profile: { display_name: 'Satoshi', about: 'Running a sovereign Bitcoin node', picture: '/demo-avatars/satoshi.svg' },
          },
        }
        return res.json({ result: identities[id] || identities['id-primary'] })
      }

      case 'identity.export-keys': {
        const { password } = params || {}
        if (password !== MOCK_PASSWORD) {
          return res.json({ error: { code: -32603, message: 'Incorrect password' } })
        }
        return res.json({
          result: {
            ed25519_secret_hex: 'mock_ed25519_secret_' + '0'.repeat(40),
            nostr_secret_hex: 'mock_nostr_secret_' + '0'.repeat(44),
            nostr_nsec: 'nsec1mockkeymockkeymockkeymockkeymockkeymockkeymockkeymockkeymo',
          },
        })
      }

      case 'identity.update-profile': {
        console.log(`[Identity] Updated profile for: ${params?.id}`)
        return res.json({ result: { success: true } })
      }

      case 'identity.publish-profile': {
        console.log(`[Identity] Published profile for: ${params?.id}`)
        return res.json({ result: { event_id: `nostr-event-${Date.now().toString(36)}` } })
      }

      // =========================================================================
      // Nostr
      // =========================================================================
      case 'nostr.list-relays': {
        return res.json({ result: { relays: [
          { url: 'wss://relay.damus.io', connected: true, enabled: true, added_at: '2026-04-02T10:00:00Z' },
          { url: 'wss://nos.lol', connected: true, enabled: true, added_at: '2026-04-02T10:00:00Z' },
          { url: 'wss://relay.snort.social', connected: true, enabled: true, added_at: '2026-04-10T08:30:00Z' },
          { url: 'wss://nostr.wine', connected: false, enabled: true, added_at: '2026-05-01T14:00:00Z' },
          { url: 'wss://relay.archipelago.lan', connected: true, enabled: false, added_at: '2026-05-20T09:15:00Z' },
        ] } })
      }
      case 'nostr.get-stats': {
        return res.json({ result: { total_relays: 5, connected_count: 3, enabled_count: 4 } })
      }
      case 'nostr.add-relay': {
        const { url } = params || {}
        console.log(`[Nostr] Added relay: ${url}`)
        return res.json({ result: { success: true } })
      }

      case 'nostr.remove-relay': {
        return res.json({ result: { success: true } })
      }

      case 'nostr.toggle-relay': {
        return res.json({ result: { success: true } })
      }

      // VPN (WireGuard) status
      case 'vpn.status': {
        return res.json({ result: {
          connected: true,
          provider: 'wireguard',
          interface: 'wg0',
          ip_address: '10.64.12.7',
          hostname: 'archipelago-demo',
          peers_connected: 3,
          bytes_in: 1_843_201_004,
          bytes_out: 642_889_310,
        } })
      }
      // Fake tunnel provisioning so the companion remote-access onboarding
      // (CompanionIntroOverlay) walks the same steps as a real node. The keys
      // are not real; importing this config yields a harmless dead tunnel.
      case 'vpn.list-peers': {
        return res.json({ result: { peers: mockState.vpnPeers || [] } })
      }
      case 'vpn.create-peer':
      case 'vpn.peer-config': {
        const peerName = params?.name || 'device'
        if (!mockState.vpnPeers) mockState.vpnPeers = []
        if (!mockState.vpnPeers.some((p) => p.name === peerName)) {
          mockState.vpnPeers.push({ name: peerName, ip: '10.44.0.7' })
        }
        const config = [
          '[Interface]',
          'PrivateKey = 2DEMOdemoDEMOdemoDEMOdemoDEMOdemoDEMOdem0=',
          'Address = 10.44.0.7/32',
          'DNS = 1.1.1.1',
          '',
          '[Peer]',
          'PublicKey = 3DEMOdemoDEMOdemoDEMOdemoDEMOdemoDEMOdem1=',
          'Endpoint = demo.archipelago-foundation.org:51820',
          'AllowedIPs = 10.44.0.0/16',
          'PersistentKeepalive = 25',
        ].join('\n')
        return res.json({ result: { config, peer_ip: '10.44.0.7' } })
      }

      // Node visibility — interactive (persisted per demo session)
      case 'network.get-visibility': {
        // No onion_address in the demo: the public showcase shouldn't display
        // a Tor address (even a fake one) on the Node Visibility card.
        return res.json({ result: {
          visibility: mockState.nodeVisibility,
        } })
      }
      case 'network.set-visibility': {
        const v = params?.visibility
        if (['hidden', 'discoverable', 'public'].includes(v)) mockState.nodeVisibility = v
        return res.json({ result: {
          visibility: mockState.nodeVisibility,
        } })
      }

      // =========================================================================
      // Content & Network
      // =========================================================================
      case 'content.browse-peer': {
        const onion = params?.onion || ''
        return res.json({ result: { items: peerCatalogFor(onion) } })
      }

      case 'content.preview-peer': {
        const entry = PEER_LIBRARY.find(it => it.id === params?.content_id)
        // Audio preview: the frontend's Preview button PLAYS these bytes (the
        // real backend proxies the seller's audio preview too), so serve a
        // leading slice of the actual track — mp3 frames tolerate truncation —
        // never an image. ~10% matches the "10% preview" badge.
        if (entry?.mime_type?.startsWith('audio/') && entry.disk) {
          try {
            const data = fsSync.readFileSync(entry.disk)
            const slice = data.subarray(0, Math.max(96 * 1024, Math.floor(data.length / 10)))
            return res.json({ result: { data: slice.toString('base64'), content_type: entry.mime_type } })
          } catch { /* fall through to no preview (icon fallback) */ }
        }
        // Serve the item's committed preview image (film poster, book cover, or
        // the photo itself) so the demo NEVER renders a broken thumbnail.
        const previewAbs = entry && (entry.preview || (entry.mime_type.startsWith('image/') && entry.disk))
        if (previewAbs) {
          try {
            const data = fsSync.readFileSync(typeof previewAbs === 'string' ? previewAbs : entry.disk)
            return res.json({ result: { data: data.toString('base64'), content_type: 'image/jpeg' } })
          } catch { /* fall through to no preview (icon fallback) */ }
        }
        return res.json({ result: {} })
      }

      case 'content.download-peer': {
        // Free items backed by a real committed file stream the actual bytes —
        // music plays, photos open, documents read. Anything else gets the
        // demo placeholder note.
        const entry = PEER_LIBRARY.find(it => it.id === params?.content_id)
        if (entry?.disk) {
          try {
            const data = fsSync.readFileSync(entry.disk)
            return res.json({ result: { data: data.toString('base64'), content_type: entry.mime_type } })
          } catch { /* fall through to placeholder */ }
        }
        const placeholder = Buffer.from(
          `Archipelago demo — "${entry?.filename || params?.content_id || 'file'}"\n\n` +
          'This is sample shared content. On a real node this would be the actual file.\n',
          'utf-8'
        ).toString('base64')
        return res.json({ result: { data: placeholder, content_type: 'text/plain' } })
      }

      case 'content.remove': {
        return res.json({ result: { success: true } })
      }

      case 'content.set-pricing': {
        return res.json({ result: { success: true } })
      }

      // ── Demo paid-content / "buying files" flow ───────────────────────────
      // Every payment path resolves to success so visitors can experience the
      // full buy → unlock journey with testnet addresses and invoices.
      case 'content.owned-list': {
        return res.json({ result: { items: sessionOwnedContent() } })
      }
      case 'content.owned-get':
      case 'content.download-peer-paid':
      case 'content.download-peer-invoice':
      case 'content.download-peer-onchain': {
        const entry = PEER_LIBRARY.find(it => it.id === params?.content_id)
        const price = params?.price_sats
          || (entry && typeof entry.access === 'object' && entry.access.paid?.price_sats)
          || 0
        // Deduct the price from the chosen rail so demo balances react.
        const paid = params?.price_sats || 0
        if (paid > 0 && method === 'content.download-peer-paid') {
          if (params?.method === 'ark') walletState.ark_sats = Math.max(0, walletState.ark_sats - paid)
          else if (params?.method === 'fedimint') {
            const fed = (mockState.federations || [])[0]
            if (fed) fed.balance_sats = Math.max(0, (fed.balance_sats || 0) - paid)
          } else walletState.ecash_sats = Math.max(0, walletState.ecash_sats - paid)
        }
        // Record the purchase (not owned-get — that's re-opening a past buy) so
        // it persists in owned-list: the frontend refreshes Owned state right
        // after paying, and the Paid Files tab shows it with price + date.
        if (method !== 'content.owned-get' && entry) {
          const owned = sessionOwnedContent()
          if (!owned.some(o => o.content_id === entry.id && o.onion === params?.onion)) {
            owned.push({
              onion: params?.onion || '',
              content_id: entry.id,
              filename: entry.filename,
              mime_type: entry.mime_type,
              size_bytes: entry.size_bytes,
              paid_sats: price,
              purchased_at: new Date().toISOString(),
            })
          }
        }
        // Real committed bytes when we have them (music, photos, docs) — this
        // is what lets a just-bought track autoplay in the bottom bar.
        if (entry?.disk) {
          try {
            const data = fsSync.readFileSync(entry.disk)
            return res.json({ result: { data: data.toString('base64'), mime_type: entry.mime_type, ecash_backend: params?.method || 'cashu' } })
          } catch { /* fall through to placeholder */ }
        }
        // Fictional catalog entries (2GB films etc.) keep the placeholder note.
        const filename = params?.filename || entry?.filename || 'demo-content'
        const body = Buffer.from(
          `Archipelago demo — "${filename}"\n\nThis is sample paid content delivered over the ` +
          `node-to-node content market. On a real node this would be the actual file you purchased.\n`,
          'utf-8'
        ).toString('base64')
        return res.json({ result: { data: body, mime_type: 'text/plain', ecash_backend: params?.method || 'cashu' } })
      }
      case 'content.request-invoice': {
        const price = params?.price_sats || 500
        const payment_hash = randomHex(32)
        invoiceRequestedAt.set(payment_hash, Date.now())
        return res.json({
          result: {
            bolt11: 'lntb' + price + '1p' + randomHex(50),
            payment_hash,
            price_sats: price,
          },
        })
      }
      case 'content.invoice-status': {
        // Demo: leave the QR on screen for ~2s before the invoice "settles".
        const at = invoiceRequestedAt.get(params?.payment_hash)
        const paid = !at || (Date.now() - at) >= 2000
        return res.json({ result: { paid } })
      }

      // ── Networking profits: per-service pricing (interactive) ──────────────
      case 'streaming.list-services': {
        return res.json({ result: { services: mockState.streamingServices || [] } })
      }
      case 'streaming.list-sessions': {
        const mkTime = (minsAgo) => new Date(Date.now() - minsAgo * 60_000).toISOString()
        return res.json({ result: {
          sessions: [
            { id: 'sess-demo-1', peer_id: 'npub1walker…k3u9', service_id: 'content-download', metric: 'bytes', allotment: 524_288_000, used: 231_211_008, paid_sats: 500, created_at: mkTime(134), last_topup_at: mkTime(22), expires_at: '', active: true },
            { id: 'sess-demo-2', peer_id: 'npub1sailor…m2xq', service_id: 'nostr-relay', metric: 'milliseconds', allotment: 10_800_000, used: 4_920_000, paid_sats: 30, created_at: mkTime(82), last_topup_at: mkTime(82), expires_at: mkTime(-98), active: true },
            { id: 'sess-demo-3', peer_id: 'npub1nomad…t7rd', service_id: 'content-download', metric: 'bytes', allotment: 104_857_600, used: 88_080_384, paid_sats: 100, created_at: mkTime(9), last_topup_at: mkTime(9), expires_at: '', active: true },
          ],
          total_active: 3,
          total_revenue_sats: 770_000,
          revenue_by_service: {
            'content-download': 512_400,
            'nostr-relay': 201_600,
            'api-access': 56_000,
          },
        } })
      }
      case 'streaming.configure-service': {
        const p = params || {}
        const list = mockState.streamingServices || []
        const idx = list.findIndex(s => s.service_id === p.service_id)
        if (idx >= 0) list[idx] = { ...list[idx], ...p }
        else list.push({ accepted_mints: [], ...p })
        mockState.streamingServices = list
        return res.json({ result: { success: true } })
      }

      // ── Wallet settings: Cashu mints + Fedimint federations (interactive) ──
      case 'streaming.list-mints': {
        return res.json({ result: { mints: mockState.cashuMints } })
      }
      case 'streaming.configure-mints': {
        if (Array.isArray(params?.mints)) mockState.cashuMints = params.mints
        return res.json({ result: { success: true, mints: mockState.cashuMints } })
      }
      case 'wallet.fedimint-list': {
        return res.json({ result: { federations: mockState.federations } })
      }
      case 'wallet.fedimint-join': {
        const fed = { federation_id: 'fed1' + randomHex(28), name: 'Joined Federation', balance_sats: 0 }
        mockState.federations.push(fed)
        return res.json({ result: { success: true, ...fed } })
      }
      case 'wallet.fedimint-balance': {
        const total = (mockState.federations || []).reduce((s, f) => s + (f.balance_sats || 0), 0)
        return res.json({ result: { balance_sats: total } })
      }
      case 'wallet.ark-status': {
        return res.json({
          result: {
            available: true,
            wallet_ready: true,
            wallet: { network: walletState.ark_config.network },
            ark_info: { server: walletState.ark_config.ark_server },
            config: walletState.ark_config,
          },
        })
      }
      case 'wallet.ark-balance': {
        return res.json({
          result: {
            balance_sats: walletState.ark_sats,
            spendable_sats: walletState.ark_sats,
            pending_sats: walletState.ark_pending_sats,
            onchain_sats: walletState.ark_onchain_sats,
          },
        })
      }
      case 'wallet.ark-address': {
        const onchain = params?.onchain === true
        const address = onchain ? 'tb1q' + randomHex(16) : 'tark1' + randomHex(28)
        return res.json({ result: { address, onchain } })
      }
      case 'wallet.ark-board': {
        const amt = params?.amount_sats ?? walletState.ark_onchain_sats
        walletState.ark_onchain_sats = Math.max(0, walletState.ark_onchain_sats - amt)
        walletState.ark_sats += amt
        return res.json({ result: { boarded_sats: amt } })
      }
      case 'wallet.ark-offboard': {
        const amt = walletState.ark_sats
        walletState.ark_sats = 0
        walletState.ark_onchain_sats += amt
        return res.json({ result: { offboarded_sats: amt } })
      }
      case 'wallet.ark-send': {
        const amt = params?.amount_sats || 1000
        walletState.ark_sats = Math.max(0, walletState.ark_sats - amt)
        return res.json({ result: { sent: true, movement: { id: 1, effective_balance_sat: -amt } } })
      }
      case 'wallet.ark-invoice': {
        return res.json({ result: { invoice: 'lntbs' + randomHex(64) } })
      }
      case 'wallet.ark-history': {
        return res.json({ result: { transactions: [] } })
      }
      case 'wallet.ark-configure': {
        const cfg = { ...walletState.ark_config }
        for (const key of ['network', 'ark_server', 'esplora']) {
          if (typeof params?.[key] === 'string' && params[key].trim()) cfg[key] = params[key].trim()
        }
        walletState.ark_config = cfg
        return res.json({ result: { config: cfg } })
      }
      case 'content.request-onchain': {
        const price = params?.price_sats || 500
        return res.json({
          result: {
            address: 'tb1q' + randomHex(19),
            amount_sats: price,
            content_id: params?.content_id || '',
            expires_at: new Date(Date.now() + 3600000).toISOString(),
          },
        })
      }
      case 'content.onchain-status': {
        return res.json({ result: { paid: true, status: 'confirmed', confirmations: 1 } })
      }

      // ── FIPS encrypted-mesh transport (shows as fully active in the demo) ──
      case 'fips.status': {
        return res.json({
          result: {
            installed: true,
            version: '0.4.2',
            service_state: 'active',
            upstream_service_state: 'active',
            service_active: true,
            key_present: true,
            npub: 'npub1' + randomHex(28),
            authenticated_peer_count: 5,
            anchor_connected: true,
          },
        })
      }
      case 'fips.list-seed-anchors': {
        return res.json({
          result: {
            seed_anchors: [
              { npub: 'npub1' + randomHex(28), address: 'fips.v0l.io:8443', transport: 'tcp', label: 'Public anchor' },
              { npub: 'npub1' + randomHex(28), address: 'anchor.archipelago.lan:8443', transport: 'tcp', label: 'Federation anchor' },
            ],
          },
        })
      }
      case 'fips.add-seed-anchor':
      case 'fips.remove-seed-anchor': {
        const seed = [
          { npub: 'npub1' + randomHex(28), address: params?.address || 'fips.v0l.io:8443', transport: 'tcp', label: params?.label || 'Anchor' },
        ]
        return res.json({ result: { seed_anchors: seed, apply: [{ npub: seed[0].npub, ok: true, message: 'applied' }] } })
      }
      case 'fips.apply-seed-anchors': {
        return res.json({ result: { applied: 2, results: [
          { npub: 'npub1' + randomHex(28), ok: true, message: 'connected' },
          { npub: 'npub1' + randomHex(28), ok: true, message: 'connected' },
        ] } })
      }
      case 'fips.reconnect': {
        return res.json({ result: { success: true, anchor_connected: true, authenticated_peer_count: 5 } })
      }
      case 'fips.install': {
        return res.json({ result: { success: true, installed: true, version: '0.4.2' } })
      }

      case 'network.diagnostics': {
        return res.json({
          result: {
            wan_ip: '82.14.203.47',
            nat_type: 'Full Cone',
            upnp_available: true,
            tor_connected: true,
            wifi_count: 3,
          },
        })
      }

      case 'network.list-interfaces': {
        return res.json({
          result: {
            interfaces: [
              { name: 'eth0', type: 'ethernet', state: 'up', mac: 'a8:a1:59:3c:f2:10', ipv4: ['192.0.2.10/24'] },
              { name: 'wlan0', type: 'wifi', state: 'up', mac: 'dc:a6:32:12:ab:cd', ipv4: ['192.0.2.14/24'] },
              { name: 'lo', type: 'loopback', state: 'up', mac: '00:00:00:00:00:00', ipv4: ['127.0.0.1/8'] },
              { name: 'podman0', type: 'bridge', state: 'up', mac: '2e:f4:8a:11:22:33', ipv4: ['10.89.0.1/16'] },
              { name: 'tailscale0', type: 'tunnel', state: 'up', mac: '', ipv4: ['100.64.0.63/32'] },
            ],
          },
        })
      }

      case 'network.scan-wifi': {
        return res.json({
          result: {
            networks: [
              { ssid: 'HomeNetwork', signal: 92, security: 'WPA2' },
              { ssid: 'Neighbor-5G', signal: 45, security: 'WPA3' },
              { ssid: 'CoffeeShop', signal: 30, security: 'Open' },
            ],
          },
        })
      }

      case 'network.configure-wifi': {
        console.log(`[Network] Connecting to WiFi: ${params?.ssid}`)
        return res.json({ result: { success: true } })
      }

      case 'network.dns-status': {
        const dns = mockState.dns || { provider: 'system', servers: ['1.1.1.1', '9.9.9.9'], doh_enabled: false }
        return res.json({
          result: {
            provider: dns.provider,
            servers: dns.servers,
            doh_enabled: dns.doh_enabled,
            doh_url: null,
            resolv_conf_servers: dns.servers,
          },
        })
      }

      case 'network.configure-dns': {
        const dnsProviders = {
          system: ['192.168.4.1'],
          cloudflare: ['1.1.1.1', '1.0.0.1'],
          google: ['8.8.8.8', '8.8.4.4'],
          quad9: ['9.9.9.9', '149.112.112.112'],
          mullvad: ['194.242.2.2'],
        }
        const provider = params?.provider || 'system'
        const servers = provider === 'custom'
          ? (Array.isArray(params?.servers) ? params.servers : [])
          : (dnsProviders[provider] || dnsProviders.system)
        const doh_enabled = ['cloudflare', 'google', 'quad9', 'mullvad'].includes(provider)
        mockState.dns = { provider, servers, doh_enabled }
        console.log(`[Network] DNS configured: ${provider} → ${servers.join(', ')}`)
        return res.json({ result: { provider, servers, doh_enabled } })
      }

      case 'network.accept-request': {
        return res.json({ result: { success: true } })
      }

      case 'network.reject-request': {
        return res.json({ result: { success: true } })
      }

      // =========================================================================
      // Server & Auth extras
      // =========================================================================
      case 'server.health': {
        return res.json({
          result: {
            status: 'healthy',
            uptime: Math.floor(process.uptime()),
            services: {
              backend: 'running',
              nginx: 'running',
              podman: 'running',
              tor: 'running',
            },
          },
        })
      }

      case 'auth.changePassword': {
        const { currentPassword } = params || {}
        if (currentPassword !== userState.passwordHash && currentPassword !== MOCK_PASSWORD) {
          return res.json({ error: { code: -32603, message: 'Current password is incorrect' } })
        }
        userState.passwordHash = params.newPassword
        console.log('[Auth] Password changed')
        return res.json({ result: { success: true } })
      }

      // =========================================================================
      // Router (port forwarding)
      // =========================================================================
      case 'router.add-forward': {
        console.log(`[Router] Added forward: ${JSON.stringify(params)}`)
        return res.json({ result: { success: true, id: `fwd-${Date.now()}` } })
      }

      case 'router.remove-forward': {
        return res.json({ result: { success: true } })
      }

      case 'router.list-forwards': {
        return res.json({
          result: {
            forwards: [
              { id: 'fwd-1', name: 'Bitcoin P2P', external_port: 8333, internal_port: 8333, protocol: 'tcp', enabled: true },
              { id: 'fwd-2', name: 'LND gRPC', external_port: 10009, internal_port: 10009, protocol: 'tcp', enabled: true },
            ],
          },
        })
      }

      // =========================================================================
      // Tor & Peer Networking
      // =========================================================================
      case 'tor.list-services': {
        mockState.torServices = mockState.torServices || defaultTorServices()
        return res.json({
          result: {
            tor_running: true,
            services: mockState.torServices,
          },
        })
      }

      case 'tor.create-service': {
        mockState.torServices = mockState.torServices || defaultTorServices()
        const name = params?.name
        if (!name) {
          return res.json({ error: { code: -1, message: 'Missing name' } })
        }
        if (mockState.torServices.some(s => s.name === name)) {
          return res.json({ error: { code: -1, message: `Service '${name}' already exists` } })
        }
        const stem = `${name.toLowerCase().replace(/[^a-z0-9]/g, '')}demo`
        const onion = (stem + 'x7k3pnw4hv5qz2jcbr6dwefys3ockqzf4mzjlvxot2ioadmnopqrstuv').slice(0, 56) + '.onion'
        const svc = { name, local_port: params?.local_port || 8080, onion_address: onion, enabled: true, unauthenticated: false, protocol: false }
        mockState.torServices = [...mockState.torServices, svc]
        console.log(`[Tor] Created hidden service: ${name} → ${onion}`)
        return res.json({ result: { created: true, name, onion_address: onion } })
      }

      case 'tor.delete-service': {
        mockState.torServices = mockState.torServices || defaultTorServices()
        const name = params?.name
        if (name === 'archipelago') {
          return res.json({ error: { code: -1, message: "Cannot delete the node's own Tor service" } })
        }
        if (!mockState.torServices.some(s => s.name === name)) {
          return res.json({ error: { code: -1, message: `Service '${name}' not found` } })
        }
        mockState.torServices = mockState.torServices.filter(s => s.name !== name)
        return res.json({ result: { deleted: true, name } })
      }

      case 'node.tor-address': {
        return res.json({
          result: {
            tor_address: 'archydemox7k3pnw4hv5qz2jcbr6dwefys3ockqzf4mzjlvxot2ioad.onion',
          },
        })
      }

      case 'node-list-peers': {
        return res.json({
          result: {
            peers: [
              { onion: 'peer1abc2def3ghi4jkl5mno6pqr7stu8vwx9yz.onion', pubkey: 'a1b2c3d4e5f6', name: 'satoshi-node' },
              { onion: 'peer2xyz9wvu8tsr7qpo6nml5kji4hgf3edc2ba.onion', pubkey: 'f6e5d4c3b2a1', name: 'lightning-lab' },
              { onion: 'peer3mno6pqr7stu8vwx9yzabc2def3ghi4jkl5.onion', pubkey: 'c3d4e5f6a1b2', name: 'sovereign-relay' },
            ],
          },
        })
      }

      case 'node-check-peer': {
        return res.json({ result: { onion: params?.onion || '', reachable: Math.random() > 0.2 } })
      }

      case 'node-add-peer': {
        console.log(`[Peers] Added peer: ${params?.onion}`)
        return res.json({ result: { success: true } })
      }

      case 'node-send-message': {
        console.log(`[Peers] Sent message to: ${params?.onion}`)
        return res.json({ result: { ok: true, sent_to: params?.onion || '' } })
      }

      case 'node-nostr-discover': {
        return res.json({
          result: {
            nodes: [
              { did: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9', onion: 'disc1abc2def3ghi4jkl5mno6pqr7stu8vwx9yz.onion', pubkey: 'disc1pub', node_address: '192.0.2.50' },
              { did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH', onion: 'disc2xyz9wvu8tsr7qpo6nml5kji4hgf3edc2ba.onion', pubkey: 'disc2pub', node_address: '192.0.2.51' },
              { did: 'did:key:z6MkfV2sQpXm4d8YtR1nWc7uHb3eKj9gLa5xPzD6oTiN8rEw', onion: 'disc3mn04pq15rs26tu37vw48xy59za60bc71de.onion', pubkey: 'disc3pub', node_address: '192.0.2.72' },
              { did: 'did:key:z6MkrJ8pWx2yNc5vT9qLb4eHu7dKf1gMa3sPzE6oXiQ8nRvw', onion: 'disc4fg82hi93jk04lm15no26pq37rs48tu59vw.onion', pubkey: 'disc4pub', node_address: '100.64.0.6' },
              { did: 'did:key:z6MkhT4wQn8xPc2vL6sRb9eYu3dJf7gKa1mNzD5oWiE8tXvq', onion: 'disc5xy60za71bc82de93fg04hi15jk26lm37no.onion', pubkey: 'disc5pub', node_address: '100.64.0.23' },
            ],
          },
        })
      }

      case 'node-messages-received':
      case 'node.messages': {
        return res.json({
          result: {
            messages: [
              { from_pubkey: 'a1b2c3d4e5f6', message: 'Hey, your relay is online! Nice uptime.', timestamp: new Date(Date.now() - 3600000).toISOString() },
              { from_pubkey: 'f6e5d4c3b2a1', message: 'Channel opened successfully. 500k sats capacity.', timestamp: new Date(Date.now() - 7200000).toISOString() },
              { from_pubkey: 'c3d4e5f6a1b2', message: 'Backup sync complete. All good on my end.', timestamp: new Date(Date.now() - 86400000).toISOString() },
            ],
          },
        })
      }

      case 'node.notifications': {
        return res.json({
          result: [
            {
              id: 'notif-1',
              type: 'info',
              title: 'Bitcoin Core synced',
              message: 'Blockchain fully synced at block 892,451. IBD complete.',
              timestamp: new Date(Date.now() - 1800000).toISOString(),
              read: false,
            },
            {
              id: 'notif-2',
              type: 'success',
              title: 'LND channel opened',
              message: 'Channel opened with ACINQ (500,000 sats capacity). 6 confirmations received.',
              timestamp: new Date(Date.now() - 7200000).toISOString(),
              read: false,
            },
            {
              id: 'notif-3',
              type: 'warning',
              title: 'Disk usage above 80%',
              message: 'Storage at 82% capacity. Consider pruning old blockchain data or expanding storage.',
              timestamp: new Date(Date.now() - 14400000).toISOString(),
              read: true,
            },
            {
              id: 'notif-4',
              type: 'info',
              title: 'System update available',
              message: 'Archipelago v0.1.1 is available. Review changelog before updating.',
              timestamp: new Date(Date.now() - 86400000).toISOString(),
              read: true,
            },
            {
              id: 'notif-5',
              type: 'success',
              title: 'Fedimint guardian connected',
              message: 'Guardian consensus achieved. Mint is operational with 3/4 guardians online.',
              timestamp: new Date(Date.now() - 43200000).toISOString(),
              read: false,
            },
          ],
        })
      }

      // =====================================================================
      // Federation (multi-node clusters)
      // =====================================================================
      case 'federation.list':
      case 'federation.list-nodes': {
        return res.json({ result: { nodes: sessionFederationNodes() } })
      }

      case 'federation.invite': {
        // trust level threads through the invite (backend parity):
        // "Invite a Peer" sends observer, "Link Your Nodes" trusted.
        const trust = params?.trust_level === 'observer' ? 'observer' : 'trusted'
        const mockCode = 'fed1:' + Buffer.from(JSON.stringify({
          did: 'did:key:z6MkTest228NodeInvite',
          onion: 'self228abc2def3ghi4jkl5mno6pqr7stu8vwx.onion',
          pubkey: 'aabbccdd',
          token: 'mock-invite-token-' + Date.now(),
          trust,
        })).toString('base64url')
        return res.json({
          result: {
            code: mockCode,
            did: 'did:key:z6MkTest228NodeInvite',
            onion: 'self228abc2def3ghi4jkl5mno6pqr7stu8vwx.onion',
            trust_level: trust,
          },
        })
      }

      case 'federation.join': {
        console.log(`[Federation] Joining with code: ${params?.code?.substring(0, 20)}...`)
        return res.json({
          result: {
            joined: true,
            node: {
              did: 'did:key:z6MkNewJoinedNode',
              onion: 'newnode123abc456def789ghi012jkl345mno6pqr.onion',
              pubkey: 'ddeeff11',
              trust_level: 'trusted',
            },
          },
        })
      }

      case 'federation.sync-state': {
        return res.json({
          result: {
            synced: 3,
            failed: 0,
            results: [
              { did: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9', status: 'synced', apps: 4 },
              { did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH', status: 'synced', apps: 3 },
              { did: 'did:key:z6MkrHKPxJP6tvCvXMaJKZd3rRA2Y44tyftVhR8FDCMKGFjb', status: 'synced', apps: 2 },
            ],
          },
        })
      }

      case 'federation.set-trust': {
        console.log(`[Federation] Set trust: ${params?.did} -> ${params?.trust_level}`)
        return res.json({ result: { updated: true, did: params?.did, trust_level: params?.trust_level } })
      }

      case 'federation.remove-node': {
        console.log(`[Federation] Remove node: ${params?.did}`)
        return res.json({ result: { removed: true, nodes_remaining: 2 } })
      }

      case 'federation.deploy-app': {
        console.log(`[Federation] Deploy app: ${params?.app_id} to ${params?.target_did}`)
        return res.json({ result: { deployed: true, app_id: params?.app_id } })
      }

      // =====================================================================
      // DWN (Decentralized Web Node)
      // =====================================================================
      case 'dwn.status': {
        return res.json({
          result: {
            running: true,
            protocols_registered: 3,
            messages_stored: 47,
            peers_synced: 2,
            last_sync: new Date(Date.now() - 600000).toISOString(),
            protocols: [
              { uri: 'https://archipelago.dev/protocols/node-identity/v1', published: true, messages: 12 },
              { uri: 'https://archipelago.dev/protocols/federation/v1', published: false, messages: 28 },
              { uri: 'https://archipelago.dev/protocols/app-deploy/v1', published: false, messages: 7 },
            ],
          },
        })
      }

      case 'dwn.sync': {
        console.log('[DWN] Syncing with peers...')
        return res.json({ result: { synced: true, messages_pulled: 5, messages_pushed: 3 } })
      }

      // =====================================================================
      // Mesh Networking (LoRa radio via Meshcore)
      // =====================================================================
      case 'mesh.rnode-config': {
        return res.json({ result: {
          settings: { enabled: true, port: null, frequency: 869525000,
            bandwidth: 125000, spreading_factor: 8, coding_rate: 5, txpower: 17,
            airtime_limit_short: null, airtime_limit_long: null },
          live: null, live_error: 'Radio hardware is unavailable in this demo.',
        } })
      }
      case 'mesh.rnode-config-apply': {
        // Never make a hardware claim or execute a radio command in the demo.
        return res.json({ result: { applied: false, confirmed: false, live: null,
          message: 'Radio settings cannot be applied in this demo. Connect to your own node to configure its radio.' } })
      }
      case 'mesh.status': {
        globalThis.__meshHeaders ||= { announce_block_headers: false, receive_block_headers: true }
        // Stateful enable/disable so the dev UI can demo the global
        // "mesh device detected" setup modal: disable mesh on the Mesh page
        // and the modal fires (device present but not connected).
        globalThis.__meshCfg ||= { enabled: true, lora_region: 'EU_868', device_kind: null, channel_name: 'archipelago', advert_name: 'archy-228' }
        const mc = globalThis.__meshCfg
        return res.json({
          result: {
            enabled: mc.enabled,
            device_type: mc.enabled ? 'Meshcore' : 'unknown',
            device_path: '/dev/ttyUSB0',
            device_connected: mc.enabled,
            firmware_version: mc.enabled ? '2.3.1' : null,
            self_node_id: mc.enabled ? 42 : null,
            self_advert_name: mc.advert_name,
            peer_count: mc.enabled ? 4 : 0,
            channel_name: mc.channel_name,
            messages_sent: 23,
            messages_received: 47,
            // Demo device-detection: ~8s into the session a second, freshly
            // "plugged-in" radio appears on /dev/ttyACM0. The frontend's
            // 2-sighting debounce then pops the global setup modal a few
            // seconds later. plugged_at is FIXED so dismissing it ("Not now")
            // sticks for the whole browser session — the modal shows once.
            detected_devices: (() => {
              mockState.meshFirstStatusAt ||= Date.now()
              const demoPlug = Date.now() - mockState.meshFirstStatusAt > 8000
              return demoPlug ? ['/dev/ttyUSB0', '/dev/ttyACM0'] : ['/dev/ttyUSB0']
            })(),
            device_present: true,
            detected_device_info: [
              { path: '/dev/ttyUSB0', vid: '10c4', pid: 'ea60', product: 'HELTEC LoRa 32 V3', manufacturer: 'Heltec', plugged_at: 1753790000 },
              ...(Date.now() - (mockState.meshFirstStatusAt || Date.now()) > 8000
                ? [{ path: '/dev/ttyACM0', vid: '303a', pid: '1001', product: 'Heltec V4 (RNode)', manufacturer: 'Espressif', plugged_at: 1753790001 }]
                : []),
            ],
            lora_region: mc.lora_region,
            device_kind: mc.device_kind,
            region: mc.enabled ? mc.lora_region : null,
            announce_block_headers: globalThis.__meshHeaders.announce_block_headers,
            receive_block_headers: globalThis.__meshHeaders.receive_block_headers,
            manage_radio: mc.manage_radio !== false,
          },
        })
      }

      case 'mesh.peers': {
        // Built from the shared DEMO_MESH_PEERS list so mesh.contacts-list
        // cannot drift from it; last_heard is aged relative to now.
        const peers = DEMO_MESH_PEERS.map((p) => ({
          ...p,
          last_heard: new Date(Date.now() - (DEMO_PEER_LAST_HEARD_MS[p.contact_id] || 0)).toISOString(),
        }))
        return res.json({ result: { peers, count: peers.length } })
      }

      case 'mesh.messages': {
        const limit = params?.limit || 100
        const now = Date.now()
        const allMessages = [
          { id: 1, direction: 'received', peer_contact_id: 1, peer_name: 'archy-198', plaintext: 'Node online. Bitcoin Knots synced to tip.', timestamp: new Date(now - 3600000).toISOString(), delivered: true, encrypted: true, transport: 'meshcore', sender_pubkey: 'demo01abababababababababababababababababababababababababababab', sender_seq: 1, message_type: 'text' },
          { id: 2, direction: 'sent', peer_contact_id: 1, peer_name: 'archy-198', plaintext: 'Good. Electrs index at 98%. Channel capacity 2.5M sats.', timestamp: new Date(now - 3540000).toISOString(), delivered: true, encrypted: true, transport: 'meshcore', sender_pubkey: 'demo02abababababababababababababababababababababababababababab', sender_seq: 2, message_type: 'text' },
          { id: 3, direction: 'received', peer_contact_id: 2, peer_name: 'satoshi-relay', plaintext: 'Block #890,413 relayed. Fees avg 12 sat/vB.', timestamp: new Date(now - 3000000).toISOString(), delivered: true, encrypted: true, transport: 'reticulum', sender_pubkey: 'demo03abababababababababababababababababababababababababababab', sender_seq: 3, message_type: 'block_header', typed_payload: { alert_type: 'block_header', message: 'Block #890,413 — 2,847 txs, 12 sat/vB avg fee', signed: true } },
          { id: 4, direction: 'received', peer_contact_id: 1, peer_name: 'archy-198', plaintext: 'Invoice: 50,000 sats — Channel opening fee', timestamp: new Date(now - 1800000).toISOString(), delivered: true, encrypted: true, transport: 'fips', sender_pubkey: 'demo04abababababababababababababababababababababababababababab', sender_seq: 4, message_type: 'invoice', typed_payload: { bolt11: 'lnbc500000n1pjmesh...truncated...', amount_sats: 50000, memo: 'Channel opening fee', paid: false } },
          { id: 5, direction: 'sent', peer_contact_id: 4, peer_name: 'bunker-alpha', plaintext: 'Running mesh-only mode. No internet for 48h. All good.', timestamp: new Date(now - 900000).toISOString(), delivered: true, encrypted: true, transport: 'reticulum', sender_pubkey: 'demo05abababababababababababababababababababababababababababab', sender_seq: 5, message_type: 'text' },
          { id: 6, direction: 'received', peer_contact_id: 4, peer_name: 'bunker-alpha', plaintext: 'Copy. Block height 890,412 via compact headers.', timestamp: new Date(now - 840000).toISOString(), delivered: true, encrypted: true, transport: 'reticulum', sender_pubkey: 'demo06abababababababababababababababababababababababababababab', sender_seq: 6, message_type: 'text' },
          { id: 7, direction: 'received', peer_contact_id: 3, peer_name: 'mountain-node', plaintext: 'EMERGENCY: Solar array failure. Running on battery reserve.', timestamp: new Date(now - 600000).toISOString(), delivered: true, encrypted: false, transport: 'meshcore', sender_pubkey: 'demo07abababababababababababababababababababababababababababab', sender_seq: 7, message_type: 'alert', typed_payload: { alert_type: 'emergency', message: 'Solar array failure. Running on battery reserve. ETA 4h before shutdown.', coordinate: { lat: 39507400, lng: -106042800, label: 'Mountain relay site' }, signed: true } },
          { id: 8, direction: 'sent', peer_contact_id: 1, peer_name: 'archy-198', plaintext: 'Opening 1M sat channel to your node. Approve?', timestamp: new Date(now - 300000).toISOString(), delivered: true, encrypted: true, transport: 'fips', sender_pubkey: 'demo08abababababababababababababababababababababababababababab', sender_seq: 8, message_type: 'text' },
          { id: 9, direction: 'received', peer_contact_id: 1, peer_name: 'archy-198', plaintext: 'Approved. Waiting for funding tx confirmation.', timestamp: new Date(now - 240000).toISOString(), delivered: true, encrypted: true, transport: 'tor', sender_pubkey: 'demo09abababababababababababababababababababababababababababab', sender_seq: 9, message_type: 'text' },
          { id: 10, direction: 'sent', peer_contact_id: 3, peer_name: 'mountain-node', plaintext: 'Location shared', timestamp: new Date(now - 120000).toISOString(), delivered: true, encrypted: false, transport: 'meshcore', sender_pubkey: 'demo10abababababababababababababababababababababababababababab', sender_seq: 10, message_type: 'coordinate', typed_payload: { lat: 30267200, lng: -97743100, label: 'Supply drop point' } },
          { id: 11, direction: 'received', peer_contact_id: 4, peer_name: 'bunker-alpha', plaintext: 'Dead man switch check-in. All systems nominal. Battery 78%.', timestamp: new Date(now - 60000).toISOString(), delivered: true, encrypted: true, transport: 'reticulum', sender_pubkey: 'demo11abababababababababababababababababababababababababababab', sender_seq: 11, message_type: 'alert', typed_payload: { alert_type: 'status', message: 'All systems nominal. Battery 78%. Mesh uptime 14d.', signed: true } },
          { id: 12, direction: 'received', peer_contact_id: 1, peer_name: 'archy-198', plaintext: 'Invoice paid: 50,000 sats', timestamp: new Date(now - 30000).toISOString(), delivered: true, encrypted: true, transport: 'fips', sender_pubkey: 'demo12abababababababababababababababababababababababababababab', sender_seq: 12, message_type: 'invoice', typed_payload: { bolt11: 'lnbc500000n1pjmesh...truncated...', amount_sats: 50000, memo: 'Channel opening fee', paid: true, payment_hash: 'a1b2c3d4e5f6...' } },
        ]
        // Messages sent this session (texts + attachments) ride after the
        // static seed so refresh-after-send shows them, same as a real node.
        const withDynamic = applyMeshOverrides([...allMessages, ...currentStore().mesh.dynamic])
        return res.json({
          result: {
            messages: withDynamic.slice(0, limit),
            count: withDynamic.length,
          },
        })
      }

      case 'mesh.send': {
        const contactId = params?.contact_id
        const message = params?.message || ''
        const peer = [
          { id: 1, name: 'archy-198', encrypted: true },
          { id: 2, name: 'satoshi-relay', encrypted: true },
          { id: 3, name: 'mountain-node', encrypted: false },
          { id: 4, name: 'bunker-alpha', encrypted: true },
        ].find(p => p.id === contactId)
        console.log(`[Mesh] Send to ${peer?.name || contactId}: ${message}`)
        const dyn = currentStore().mesh.dynamic
        const id = 100 + dyn.length
        dyn.push({
          id, direction: 'sent', peer_contact_id: contactId, peer_name: peer?.name || `peer-${contactId}`,
          plaintext: message, timestamp: new Date().toISOString(), delivered: true,
          encrypted: peer?.encrypted ?? false, transport: 'meshcore',
          sender_pubkey: 'demo02abababababababababababababababababababababababababababab',
          sender_seq: id, message_type: 'text',
        })
        return res.json({
          result: {
            sent: true,
            message_id: id,
            encrypted: peer?.encrypted ?? false,
          },
        })
      }

      case 'mesh.broadcast': {
        console.log('[Mesh] Broadcasting identity over LoRa')
        return res.json({ result: { broadcast: true } })
      }

      case 'mesh.configure': {
        console.log(`[Mesh] Configure:`, params)
        globalThis.__meshHeaders ||= { announce_block_headers: false, receive_block_headers: true }
        if (params && typeof params.announce_block_headers === 'boolean') globalThis.__meshHeaders.announce_block_headers = params.announce_block_headers
        if (params && typeof params.receive_block_headers === 'boolean') globalThis.__meshHeaders.receive_block_headers = params.receive_block_headers
        // Stateful radio settings (see mesh.status) — lets the dev UI demo
        // the detected-device modal + the Device panel settings round-trip.
        globalThis.__meshCfg ||= { enabled: true, lora_region: 'EU_868', device_kind: null, channel_name: 'archipelago', advert_name: 'archy-228' }
        const cfg = globalThis.__meshCfg
        if (params && typeof params.enabled === 'boolean') cfg.enabled = params.enabled
        if (params && typeof params.lora_region === 'string') cfg.lora_region = params.lora_region || null
        if (params && typeof params.device_kind === 'string') cfg.device_kind = params.device_kind === 'auto' ? null : params.device_kind
        if (params && typeof params.channel_name === 'string') cfg.channel_name = params.channel_name
        if (params && typeof params.advert_name === 'string') cfg.advert_name = params.advert_name
        // Hot-swap "keep as is" (2026-07-22): false = never write config to the radio.
        if (params && typeof params.manage_radio === 'boolean') cfg.manage_radio = params.manage_radio
        return res.json({ result: { configured: true, enabled: cfg.enabled, lora_region: cfg.lora_region, device_kind: cfg.device_kind, manage_radio: cfg.manage_radio !== false, ...globalThis.__meshHeaders } })
      }

      // Read-only firmware probe backing the hot-swap device modal's
      // current-details card (2026-07-22). Real backend opens the serial
      // port; the demo answers instantly with what's "flashed" on the
      // Heltec that mesh.status advertises.
      case 'mesh.probe-device': {
        // 900ms delay lets the modal's "Reading what's on the radio…" spinner show.
        setTimeout(() => res.json({
          result: {
            path: params?.path || '/dev/ttyUSB0',
            kind: 'meshcore',
            firmware_version: 'MeshCore v2.3.1',
            node_id: 42,
            advert_name: 'archy-228',
            region: null,
            modem_preset: null,
            primary_channel: null,
            secondary_channel: null,
            max_contacts: 100,
          },
        }), 900)
        return
      }

      case 'mesh.send-invoice': {
        console.log(`[Mesh] Send invoice: ${params?.amount_sats} sats to contact ${params?.contact_id}`)
        return res.json({
          result: {
            sent: true,
            message_id: Math.floor(Math.random() * 10000) + 200,
            amount_sats: params?.amount_sats,
            bolt11: `lnbc${params?.amount_sats}n1pjmesh...`,
          },
        })
      }

      case 'mesh.send-coordinate': {
        console.log(`[Mesh] Send coordinate: ${params?.lat}, ${params?.lng} to contact ${params?.contact_id}`)
        return res.json({
          result: {
            sent: true,
            message_id: Math.floor(Math.random() * 10000) + 300,
            lat: Math.round((params?.lat || 0) * 1000000),
            lng: Math.round((params?.lng || 0) * 1000000),
          },
        })
      }

      case 'mesh.send-alert': {
        console.log(`[Mesh] Send alert: ${params?.alert_type} — ${params?.message}`)
        return res.json({
          result: {
            sent: true,
            alert_type: params?.alert_type || 'status',
            signed: true,
          },
        })
      }

      case 'mesh.outbox': {
        return res.json({
          result: {
            messages: [
              {
                id: 1,
                dest_did: 'did:key:z6MkrHKPxJP6tvCvXMaJKZd3rRA2Y44tyftVhR8FDCMKGFjb',
                from_did: 'did:key:z6MkSelf',
                created_at: new Date(Date.now() - 1800000).toISOString(),
                ttl_secs: 86400,
                retry_count: 3,
                relay_hops: 0,
                expired: false,
              },
              {
                id: 2,
                dest_did: 'did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp7NQD5EjEREWh',
                from_did: 'did:key:z6MkSelf',
                created_at: new Date(Date.now() - 7200000).toISOString(),
                ttl_secs: 86400,
                retry_count: 8,
                relay_hops: 1,
                expired: false,
              },
            ],
            count: 2,
          },
        })
      }

      case 'mesh.session-status': {
        const hasSess = (params?.contact_id === 1 || params?.contact_id === 4)
        return res.json({
          result: {
            has_session: hasSess,
            forward_secrecy: hasSess,
            message_count: hasSess ? 23 : 0,
            ratchet_generation: hasSess ? 7 : 0,
            peer_did: hasSess ? 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9' : null,
          },
        })
      }

      case 'mesh.rotate-prekeys': {
        console.log('[Mesh] Rotating prekeys...')
        return res.json({
          result: {
            rotated: true,
            signed_prekey_id: Math.floor(Math.random() * 1000000),
            one_time_prekeys: 10,
          },
        })
      }

      case 'mesh.deadman-status': {
        return res.json({
          result: {
            dead_man_enabled: false,
            dead_man_interval_secs: 21600,
            time_remaining_secs: 21600,
            triggered: false,
            has_gps: false,
            emergency_contacts: 2,
          },
        })
      }

      case 'mesh.deadman-configure': {
        const { enabled, interval_secs } = params || {}
        console.log(`[Mesh] Deadman configured: enabled=${enabled}, interval=${interval_secs}`)
        return res.json({
          result: {
            dead_man_enabled: enabled ?? false,
            dead_man_interval_secs: interval_secs ?? 21600,
            time_remaining_secs: interval_secs ?? 21600,
            triggered: false,
            has_gps: false,
            emergency_contacts: 2,
          },
        })
      }

      case 'mesh.deadman-checkin': {
        return res.json({
          result: {
            checked_in: true,
            time_remaining_secs: 21600,
          },
        })
      }

      case 'mesh.block-headers': {
        return res.json({
          result: {
            headers: [
              { height: 893421, hash: '0000000000000000000234a6b12dc03e5c4f7e891d2f34b5a678cd9012345678', timestamp: new Date(Date.now() - 600000).toISOString() },
              { height: 893420, hash: '00000000000000000001bc7d89ef01234567890abcdef1234567890abcdef12', timestamp: new Date(Date.now() - 1200000).toISOString() },
              { height: 893419, hash: '00000000000000000003ef4a56789012bcdef34567890abcdef1234567890ab', timestamp: new Date(Date.now() - 1800000).toISOString() },
            ],
            latest_height: 893421,
            count: 3,
          },
        })
      }

      case 'mesh.relay-tx': {
        return res.json({
          result: {
            request_id: Math.floor(Math.random() * 10000),
            queued: true,
            tx_hex_len: (params?.tx_hex || '').length,
          },
        })
      }

      case 'mesh.relay-status': {
        return res.json({
          result: {
            relayed: true,
            pending_count: 0,
            status: 'confirmed',
            txid: 'a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
          },
        })
      }

      case 'mesh.relay-lightning': {
        return res.json({
          result: {
            request_id: Math.floor(Math.random() * 10000),
            queued: true,
            amount_sats: params?.amount_sats || 0,
          },
        })
      }

      // =====================================================================
      // Transport Layer (unified routing: mesh > lan > tor)
      // =====================================================================
      case 'transport.status': {
        return res.json({
          result: {
            transports: [
              { kind: 'mesh', available: true },
              { kind: 'lan', available: true },
              { kind: 'tor', available: true },
            ],
            mesh_only: false,
            peer_count: 5,
          },
        })
      }

      case 'transport.peers': {
        return res.json({
          result: {
            peers: [
              {
                did: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9',
                pubkey_hex: 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
                name: 'archy-198',
                trust_level: 'trusted',
                mesh_contact_id: 1,
                lan_address: '192.0.2.11:5678',
                onion_address: 'peer1abc2def3ghi4jkl5mno6pqr7stu8vwx9yz.onion',
                preferred_transport: 'lan',
                available_transports: ['mesh', 'lan', 'tor'],
                last_seen: new Date(Date.now() - 30000).toISOString(),
              },
              {
                did: 'did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH',
                pubkey_hex: 'f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5d4c3b2a1f6e5',
                name: 'satoshi-relay',
                trust_level: 'trusted',
                mesh_contact_id: 2,
                lan_address: null,
                onion_address: 'peer2xyz9wvu8tsr7qpo6nml5kji4hgf3edc2ba.onion',
                preferred_transport: 'mesh',
                available_transports: ['mesh', 'tor'],
                last_seen: new Date(Date.now() - 120000).toISOString(),
              },
              {
                did: 'did:key:z6MkrHKPxJP6tvCvXMaJKZd3rRA2Y44tyftVhR8FDCMKGFjb',
                pubkey_hex: 'c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4',
                name: 'bunker-alpha',
                trust_level: 'observer',
                mesh_contact_id: 4,
                lan_address: null,
                onion_address: null,
                preferred_transport: 'mesh',
                available_transports: ['mesh'],
                last_seen: new Date(Date.now() - 45000).toISOString(),
              },
              {
                did: 'did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG',
                pubkey_hex: 'd4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5',
                name: 'office-node',
                trust_level: 'trusted',
                mesh_contact_id: null,
                lan_address: '192.0.2.42:5678',
                onion_address: 'peer4mno6pqr7stu8vwx9yzabc2def3ghi4jkl5.onion',
                preferred_transport: 'lan',
                available_transports: ['lan', 'tor'],
                last_seen: new Date(Date.now() - 60000).toISOString(),
              },
              {
                did: 'did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp7NQD5EjEREWh',
                pubkey_hex: 'e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6',
                name: 'remote-cabin',
                trust_level: 'trusted',
                mesh_contact_id: null,
                lan_address: null,
                onion_address: 'peer5xyz9abc2def3ghi4jkl5mno6pqr7stu8vw.onion',
                preferred_transport: 'tor',
                available_transports: ['tor'],
                last_seen: new Date(Date.now() - 300000).toISOString(),
              },
            ],
          },
        })
      }

      case 'transport.send': {
        const targetDid = params?.did
        console.log(`[Transport] Send to ${targetDid} via best transport`)
        return res.json({
          result: {
            sent: true,
            transport_used: 'mesh',
            did: targetDid,
          },
        })
      }

      case 'transport.set-mode': {
        const meshOnly = params?.mesh_only ?? false
        console.log(`[Transport] Set mesh_only mode: ${meshOnly}`)
        return res.json({ result: { mesh_only: meshOnly, configured: true } })
      }

      // =====================================================================
      // LND / Lightning
      // =====================================================================
      case 'lnd.getinfo': {
        return res.json({
          result: {
            alias: 'archy-signet',
            color: '#f7931a',
            num_active_channels: 4,
            num_inactive_channels: 1,
            num_pending_channels: 1,
            block_height: walletState.block_height,
            synced_to_chain: true,
            synced_to_graph: true,
            version: '0.17.4-beta',
            identity_pubkey: '03a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
            chains: [{ chain: 'bitcoin', network: 'signet' }],
            balance_sats: walletState.onchain_sats,
            channel_balance_sats: walletState.channel_sats,
          },
        })
      }

      // Lightning credential rotation (Settings → Lightning credentials).
      // Stateful so the dev preview can exercise the whole arc without a node:
      // request a rotation and the steps advance on each poll, ending with the
      // BTCPay reconnect. Digests only — the real daemon never returns a
      // macaroon and neither does this.
      case 'lnd.macaroon-status': {
        return res.json({
          result: {
            installed: true,
            admin_macaroon_sha256: '52219e90aeba8ac6a98fdac1cc754fe0a2e8407ae6758ffe2cf92039503f5575',
            issued_at: '2026-08-08 06:03:11',
            identity_pubkey: '03a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
            channels_open: 5,
            channels_pending: 1,
            lnd_error: null,
            btcpay_uses_internal_lnd: true,
            // Flip to false to see the "BTCPay is holding an old credential"
            // warning this feature exists to prevent.
            btcpay_credential_current: true,
            rotation: macaroonRotationSnapshot(),
          },
        })
      }

      case 'lnd.rotate-macaroons': {
        if (!params?.password) {
          return res.json({ error: { code: -1, message: 'Node password required to rotate Lightning credentials' } })
        }
        if (params.password !== userState.passwordHash && params.password !== MOCK_PASSWORD) {
          return res.json({ error: { code: -1, message: 'Password verification failed' } })
        }
        if (macaroonRotation.running) {
          return res.json({ error: { code: -1, message: 'A macaroon rotation is already running on this node' } })
        }
        startMacaroonRotation()
        return res.json({ result: { status: 'started' } })
      }

      case 'lnd.macaroon-rotation-progress': {
        return res.json({ result: macaroonRotationSnapshot() })
      }

      case 'lnd.gettransactions': {
        const pending = walletState.transactions.filter(tx => tx.direction === 'incoming' && tx.num_confirmations < 3).length
        return res.json({
          result: {
            transactions: walletState.transactions,
            incoming_pending_count: pending,
          },
        })
      }

      case 'lnd.channelbalance': {
        return res.json({
          result: {
            local_balance: { sat: walletState.channel_sats },
            remote_balance: { sat: 11750000 },
            pending_open_local_balance: { sat: 500000 },
          },
        })
      }

      case 'lnd.walletbalance': {
        return res.json({
          result: {
            total_balance: walletState.onchain_sats + 100000,
            confirmed_balance: walletState.onchain_sats,
            unconfirmed_balance: 100000,
          },
        })
      }

      case 'lnd.listchannels': {
        // Shape matches the real backend: status + channel_point are required
        // by the channels panel; totals feed the liquidity summary tiles.
        const channels = [
          { chan_id: '840921088114688', remote_pubkey: '031b301307574bbe9b9ac7b79cbe1700e31e544513eae0b5d7497483083f99e581', capacity: 1500000, local_balance: 950000, remote_balance: 550000, active: true, status: 'active', channel_point: randomHex(32) + ':0', peer_alias: 'Olympus by ZEUS' },
          { chan_id: '840921088114689', remote_pubkey: '03abcdef12345678901234567890123456789012345678901234567890abcdef12', capacity: 2000000, local_balance: 1200000, remote_balance: 800000, active: true, status: 'active', channel_point: randomHex(32) + ':1', peer_alias: 'WalletOfSatoshi' },
          { chan_id: '840921088114690', remote_pubkey: '02fedcba98765432109876543210987654321098765432109876543210fedcba98', capacity: 10000000, local_balance: 4500000, remote_balance: 5500000, active: true, status: 'active', channel_point: randomHex(32) + ':0', peer_alias: 'Voltage' },
          { chan_id: '840921088114691', remote_pubkey: '03456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123', capacity: 3000000, local_balance: 100000, remote_balance: 2900000, active: false, status: 'inactive', channel_point: randomHex(32) + ':0', peer_alias: 'Kraken' },
          { chan_id: '', remote_pubkey: '028d98b9969fbed53784a36617eb489a59ab6dc9b9d77fcdca9ff55307cd98e3c4', capacity: 500000, local_balance: 500000, remote_balance: 0, active: false, status: 'pending_open', channel_point: randomHex(32) + ':0', peer_alias: 'ACINQ' },
          { chan_id: '', remote_pubkey: '02c16cca44562b590dd279c942200bdccfd4f990c3a69fad620c10ef2f8228eaff', capacity: 800000, local_balance: 350000, remote_balance: 450000, active: false, status: 'closing', channel_point: randomHex(32) + ':0', closing_txid: randomHex(32), peer_alias: 'Bitrefill' },
        ]
        return res.json({
          result: {
            channels,
            // Totals sum open channels only, matching the real backend.
            total_outbound: channels.filter(c => ['active', 'inactive'].includes(c.status)).reduce((s, c) => s + c.local_balance, 0),
            total_inbound: channels.filter(c => ['active', 'inactive'].includes(c.status)).reduce((s, c) => s + c.remote_balance, 0),
          },
        })
      }

      case 'lnd.closedchannels': {
        return res.json({
          result: {
            channels: [
              { chan_id: '840921088110001', remote_pubkey: '03864ef025fde8fb587d989186ce6a4a186895ee44a926bfc370e2c366597a3f8f', capacity: 1000000, settled_balance: 612000, close_type: 'COOPERATIVE_CLOSE', closing_tx_hash: randomHex(32), channel_point: randomHex(32) + ':0', close_height: 903112 },
              { chan_id: '840921088110002', remote_pubkey: '0298f6074a454a1f5345cb2a7c6f9fce206cd0bf675d177cdbf0ca7508dd28852f', capacity: 250000, settled_balance: 0, close_type: 'REMOTE_FORCE_CLOSE', closing_tx_hash: randomHex(32), channel_point: randomHex(32) + ':1', close_height: 897540 },
            ],
          },
        })
      }

      case 'lnd.newaddress': {
        const addrType = params?.type || 'p2wkh'
        const mockAddr = addrType === 'p2tr'
          ? 'tb1p' + randomHex(29)
          : 'tb1q' + randomHex(19)
        return res.json({ result: { address: mockAddr } })
      }

      case 'lnd.addinvoice':
      case 'lnd.createinvoice': {
        const amt = params?.amt || params?.value || params?.amount_sats || 1000
        const rHash = randomHex(32)
        return res.json({
          result: {
            r_hash: rHash,
            payment_request: `lnsb${amt}n1pjmock${Date.now().toString(36)}qqqxqyz5vqsp5mock${rHash.slice(0,20)}`,
            add_index: Date.now(),
            payment_addr: randomHex(32),
          },
        })
      }

      case 'lnd.payinvoice':
      case 'lnd.sendpayment': {
        const amt = params?.amt || params?.amount_sats || 1000
        const fee = Math.floor(Math.random() * 10) + 1
        walletState.channel_sats = Math.max(0, walletState.channel_sats - amt - fee)
        return res.json({
          result: {
            payment_hash: randomHex(32),
            payment_preimage: randomHex(32),
            status: 'SUCCEEDED',
            fee_sat: fee,
            value_sat: amt,
          },
        })
      }

      case 'lnd.sendcoins': {
        // send_all sweeps the entire on-chain balance (minus a mock fee)
        const amt = params?.send_all
          ? Math.max(0, walletState.onchain_sats - 250)
          : (params?.amount || params?.amt || 50000)
        walletState.onchain_sats = Math.max(0, walletState.onchain_sats - amt)
        const txid = randomHex(32)
        walletState.transactions.unshift({
          tx_hash: txid, amount_sats: -amt, direction: 'outgoing', num_confirmations: 0,
          block_height: 0, time_stamp: Math.floor(Date.now()/1000), label: 'Sent on-chain',
          total_fees: 250, dest_addresses: [params?.addr || ''],
        })
        return res.json({
          result: { txid, amount: amt },
        })
      }

      case 'lnd.estimatefee': {
        // ~141 vB P2WPKH spend at a rate that scales with urgency
        const target = params?.target_conf || 6
        const rate = target <= 1 ? 22 : target <= 6 ? 8 : 2
        return res.json({ result: { fee_sat: rate * 141, sat_per_vbyte: rate } })
      }

      case 'lnd.decodepayreq': {
        return res.json({
          result: {
            destination: '03a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
            num_satoshis: params?.pay_req?.match(/lnsb(\d+)/)?.[1] || '1000',
            description: 'Mock decoded invoice',
            expiry: 3600,
            timestamp: Math.floor(Date.now() / 1000),
          },
        })
      }

      case 'lnd.openchannel': {
        return res.json({
          result: {
            funding_txid: Array.from({length: 32}, () => Math.floor(Math.random()*256).toString(16).padStart(2,'0')).join(''),
            output_index: 0,
          },
        })
      }

      case 'lnd.closechannel': {
        return res.json({
          result: {
            closing_txid: Array.from({length: 32}, () => Math.floor(Math.random()*256).toString(16).padStart(2,'0')).join(''),
          },
        })
      }

      case 'lnd.listinvoices': {
        return res.json({ result: { invoices: MOCK_LND_DATA.invoices } })
      }

      case 'lnd.listpayments': {
        return res.json({ result: { payments: MOCK_LND_DATA.payments } })
      }

      case 'lnd.create-psbt':
      case 'lnd.finalize-psbt':
      case 'lnd.create-raw-tx': {
        return res.json({
          result: {
            psbt: 'cHNidP8BAH0CAAAA...mockPSBT',
            txid: Array.from({length: 32}, () => Math.floor(Math.random()*256).toString(16).padStart(2,'0')).join(''),
          },
        })
      }

      // =====================================================================
      // Wallet / Ecash (Fedimint)
      // =====================================================================
      case 'wallet.ecash-balance': {
        const fedSats = (mockState.federations || []).reduce((s, f) => s + (f.balance_sats || 0), 0)
        return res.json({
          result: {
            balance_sats: walletState.ecash_sats,
            balance_msat: walletState.ecash_sats * 1000,
            cashu_sats: walletState.ecash_sats,
            fedimint_sats: fedSats,
            ark_sats: walletState.ark_sats,
            total_sats: walletState.ecash_sats + fedSats + walletState.ark_sats,
            token_count: walletState.ecash_tokens,
            federations: [
              { federation_id: 'fed1-demo', name: 'Archy Signet Mint', balance_msat: walletState.ecash_sats * 1000, gateway_active: true },
            ],
          },
        })
      }

      case 'wallet.ecash-send': {
        const amt = params?.amount_sats || 1000
        walletState.ecash_sats = Math.max(0, walletState.ecash_sats - amt)
        walletState.ecash_tokens = Math.max(0, walletState.ecash_tokens - 1)
        return res.json({
          result: {
            token: `cashuSend_mock_${amt}_${Date.now().toString(36)}`,
            amount_sats: amt,
          },
        })
      }

      case 'wallet.ecash-receive': {
        const amt = 5000
        walletState.ecash_sats += amt
        walletState.ecash_tokens += 1
        return res.json({
          result: {
            amount_sats: amt,
            federation_id: 'fed1-demo',
          },
        })
      }

      case 'wallet.ecash-history': {
        return res.json({
          result: {
            transactions: [
              { type: 'receive', amount_sats: 50000, timestamp: new Date(Date.now() - 86400000).toISOString(), note: 'Minted from Lightning' },
              { type: 'send', amount_sats: 5000, timestamp: new Date(Date.now() - 43200000).toISOString(), note: 'Sent ecash token' },
              { type: 'receive', amount_sats: 10000, timestamp: new Date(Date.now() - 3600000).toISOString(), note: 'Redeemed token' },
              // Ark movement in the unified history shape (see wallet.ark-history / load_ark_txs)
              { id: 'ark-1', tx_type: 'receive', type: 'receive', amount_sats: 75000, timestamp: new Date(Date.now() - 7200000).toISOString(), description: 'Received via Ark', note: 'Received via Ark', mint_url: '', peer: '', kind: 'ark' },
            ],
          },
        })
      }

      case 'wallet.networking-profits': {
        // Deterministic-but-varied week of profit events for the dashboard
        // chart (source/timestamp/sats mirror wallet::profits::ProfitEntry).
        const profitSources = ['streaming_revenue', 'content_sale', 'routing_fee']
        const profitNotes = ['Paid streaming session', 'Ecash content sale', 'Lightning routing fees']
        const recent = []
        const nowMs = Date.now()
        for (let i = 0; i < 42; i++) {
          const daysAgo = (i * 3) % 7
          const hour = (i * 5) % 24
          recent.push({
            source: profitSources[i % 3],
            amount_sats: 800 + ((i * 7919) % 14000),
            timestamp: new Date(nowMs - daysAgo * 86_400_000 - hour * 3_600_000).toISOString(),
            description: profitNotes[i % 3],
          })
        }
        recent.sort((a, b) => b.timestamp.localeCompare(a.timestamp))
        return res.json({
          result: {
            total_sats: 5_231_978,
            content_sales_sats: 3_180_000,
            routing_fees_sats: 1_281_978,
            streaming_revenue_sats: 770_000,
            recent,
            // legacy aliases kept for older UI builds
            relay_sats: 770_000,
            total_earned_sats: 5_231_978,
            total_forwarded_sats: 1_281_978,
            forward_count: 1284,
            period_days: 30,
            daily_avg_sats: 174_399,
          },
        })
      }

      case 'dev.faucet': {
        const amount = params?.amount_sats || 1_000_000
        const ecashAmt = Math.floor(amount / 10)
        walletState.onchain_sats += amount
        walletState.channel_sats += amount
        walletState.ecash_sats += ecashAmt
        walletState.ecash_tokens += 1
        const txid = randomHex(32)
        walletState.transactions.unshift({
          tx_hash: txid, amount_sats: amount, direction: 'incoming', num_confirmations: 0,
          block_height: 0, time_stamp: Math.floor(Date.now()/1000), label: 'Dev faucet',
          total_fees: 0, dest_addresses: [],
        })
        console.log(`[Dev Faucet] +${amount} on-chain, +${amount} Lightning, +${ecashAmt} ecash`)
        return res.json({
          result: {
            onchain: { txid, amount_sats: amount },
            lightning: { payment_hash: randomHex(32), amount_sats: amount },
            ecash: { token: `cashuSend_faucet_${amount}_${Date.now().toString(36)}`, amount_sats: ecashAmt },
            message: `Added ${amount} sats on-chain, ${amount} sats Lightning, ${ecashAmt} sats ecash`,
          },
        })
      }

      case 'bitcoin.getinfo': {
        // Demo IBD simulation: the first call of a session arms a ~90s ramp
        // from 98.2% → 100% so the setup wizard can demo the live sync timer
        // and the "finish setup" toast that fires when IBD completes.
        // (The real backend returns { block_height, sync_progress } — a 0–1
        // fraction — which is what the frontend reads; the bitcoin-core-style
        // fields are kept for any legacy consumers.)
        if (!walletState.ibd_started_at) walletState.ibd_started_at = Date.now()
        const IBD_RAMP_MS = 90_000
        const elapsed = Date.now() - walletState.ibd_started_at
        const syncProgress = Math.min(1, 0.982 + 0.018 * (elapsed / IBD_RAMP_MS))
        const tipHeight = 892451
        const height = Math.round(tipHeight * syncProgress)
        return res.json({
          result: {
            chain: 'signet',
            block_height: height,
            sync_progress: syncProgress,
            blocks: height,
            headers: tipHeight,
            bestblockhash: 'a1b2c3d4e5f6' + '0'.repeat(58),
            difficulty: 0.001126515290698186,
            mediantime: Math.floor(Date.now() / 1000) - 300,
            verificationprogress: syncProgress,
            chainwork: '000000000000000000000000000000000000000000000000000000000001a2b3',
            size_on_disk: 210_000_000,
            pruned: false,
            network: 'signet',
          },
        })
      }

      // =====================================================================
      // Analytics / Telemetry
      // =====================================================================
      case 'analytics.get-status': {
        return res.json({ result: { enabled: mockState.analyticsEnabled || false, description: 'Anonymous aggregate statistics. No personal data collected.' } })
      }
      case 'analytics.enable': {
        mockState.analyticsEnabled = true
        return res.json({ result: { enabled: true } })
      }
      case 'analytics.disable': {
        mockState.analyticsEnabled = false
        return res.json({ result: { enabled: false } })
      }
      case 'telemetry.fleet-status': {
        return res.json({
          result: {
            nodes: [
              {
                node_id: 'archy-228',
                version: '0.1.0',
                uptime_secs: 604800,
                cpu_cores: 4,
                cpu_pct: +(15 + Math.random() * 20).toFixed(1),
                mem_pct: +(38 + Math.random() * 10).toFixed(1),
                disk_pct: 34.2,
                container_count: 12,
                running_count: 10,
                federation_peers: 4,
                recent_alerts: [],
                containers: [
                  { id: 'bitcoin', state: 'running', version: '27.0' },
                  { id: 'lnd', state: 'running', version: '0.18.0' },
                  { id: 'electrs', state: 'running', version: '0.10.6' },
                  { id: 'mempool', state: 'running', version: '3.0.0' },
                ],
                reported_at: new Date().toISOString(),
              },
              {
                node_id: 'archy-198',
                version: '0.1.0',
                uptime_secs: 259200,
                cpu_cores: 4,
                cpu_pct: +(8 + Math.random() * 12).toFixed(1),
                mem_pct: +(25 + Math.random() * 8).toFixed(1),
                disk_pct: 22.7,
                container_count: 8,
                running_count: 7,
                federation_peers: 4,
                recent_alerts: [{ rule: 'container_crash', message: 'electrs restarted 2x in 1h', timestamp: new Date(Date.now() - 7200000).toISOString() }],
                containers: [
                  { id: 'bitcoin', state: 'running', version: '27.0' },
                  { id: 'lnd', state: 'running', version: '0.18.0' },
                  { id: 'electrs', state: 'running', version: '0.10.6' },
                ],
                reported_at: new Date(Date.now() - 60000).toISOString(),
              },
              {
                node_id: 'arch-1',
                version: '0.1.0',
                uptime_secs: 172800,
                cpu_cores: 2,
                cpu_pct: +(5 + Math.random() * 10).toFixed(1),
                mem_pct: +(45 + Math.random() * 15).toFixed(1),
                disk_pct: 61.3,
                container_count: 6,
                running_count: 6,
                federation_peers: 4,
                recent_alerts: [],
                containers: [
                  { id: 'bitcoin', state: 'running', version: '27.0' },
                  { id: 'lnd', state: 'running', version: '0.18.0' },
                ],
                reported_at: new Date(Date.now() - 120000).toISOString(),
              },
              {
                node_id: 'arch-2',
                version: '0.1.0',
                uptime_secs: 86400,
                cpu_cores: 2,
                cpu_pct: +(3 + Math.random() * 8).toFixed(1),
                mem_pct: +(30 + Math.random() * 10).toFixed(1),
                disk_pct: 18.9,
                container_count: 5,
                running_count: 5,
                federation_peers: 4,
                recent_alerts: [],
                containers: [
                  { id: 'bitcoin', state: 'running', version: '27.0' },
                ],
                reported_at: new Date(Date.now() - 300000).toISOString(),
              },
              {
                node_id: 'arch-3',
                version: '0.1.0',
                uptime_secs: 43200,
                cpu_cores: 4,
                cpu_pct: +(20 + Math.random() * 15).toFixed(1),
                mem_pct: +(55 + Math.random() * 10).toFixed(1),
                disk_pct: 47.5,
                container_count: 10,
                running_count: 9,
                federation_peers: 4,
                recent_alerts: [{ rule: 'disk_warning', message: 'Disk usage approaching 50%', timestamp: new Date(Date.now() - 3600000).toISOString() }],
                containers: [
                  { id: 'bitcoin', state: 'running', version: '27.0' },
                  { id: 'lnd', state: 'running', version: '0.18.0' },
                  { id: 'electrs', state: 'running', version: '0.10.6' },
                  { id: 'mempool', state: 'running', version: '3.0.0' },
                ],
                reported_at: new Date(Date.now() - 30000).toISOString(),
              },
            ],
          },
        })
      }

      case 'telemetry.fleet-alerts': {
        return res.json({
          result: {
            alerts: [
              { node_id: 'archy-198', rule: 'container_crash', message: 'electrs restarted 2x in 1h', timestamp: new Date(Date.now() - 7200000).toISOString() },
              { node_id: 'arch-3', rule: 'disk_warning', message: 'Disk usage approaching 50%', timestamp: new Date(Date.now() - 3600000).toISOString() },
              { node_id: 'arch-1', rule: 'mem_high', message: 'Memory usage above 60%', timestamp: new Date(Date.now() - 86400000).toISOString() },
            ],
          },
        })
      }

      case 'telemetry.fleet-node-history': {
        const nodeId = params?.node_id || 'archy-228'
        const now = Date.now()
        const history = Array.from({ length: 24 }, (_, i) => ({
          timestamp: new Date(now - (23 - i) * 3600000).toISOString(),
          cpu_pct: +(10 + Math.random() * 30).toFixed(1),
          mem_pct: +(30 + Math.random() * 20).toFixed(1),
          disk_pct: +(30 + Math.random() * 5).toFixed(1),
        }))
        return res.json({ result: { history } })
      }

      case 'analytics.get-snapshot':
      case 'telemetry.report': {
        return res.json({ result: {
          node_id: 'mock-dev-node',
          version: '1.2.0-alpha',
          uptime_secs: 86400,
          cpu_cores: 4,
          ram_mb: 16384,
          container_count: 12,
          running_count: 10,
          federation_peers: 2,
          recent_alerts: [],
          reported_at: new Date().toISOString(),
        }})
      }

      // System / Network / Updates
      // =====================================================================
      case 'system.stats': {
        return res.json({
          result: {
            cpu_usage_percent: +(12 + Math.random() * 18).toFixed(1),
            mem_used_bytes: 6_200_000_000 + Math.floor(Math.random() * 500_000_000),
            mem_total_bytes: 16_000_000_000,
            disk_used_bytes: 620_000_000_000 + Math.floor(Math.random() * 10_000_000_000),
            disk_total_bytes: 1_800_000_000_000,
            uptime_secs: Math.floor(process.uptime()) + 604800,
            load_avg: [+(0.5 + Math.random() * 1.5).toFixed(2), +(0.8 + Math.random()).toFixed(2), +(0.6 + Math.random()).toFixed(2)],
            net_rx_bytes: 12_400_000_000,
            net_tx_bytes: 8_900_000_000,
          },
        })
      }

      case 'update.status': {
        return res.json({
          result: {
            current_version: APP_VERSION,
            latest_version: APP_VERSION,
            update_available: false,
            release_notes: 'You are running the latest demo build.',
            channel: 'stable',
          },
        })
      }

      case 'seed.reveal': {
        if (!params || !params.password) {
          return res.json({ error: { code: -32000, message: 'Password is required to reveal the recovery phrase' } })
        }
        // Demo gate: accept any non-empty password; reject "wrong" to exercise the error path.
        if (params.password === 'wrong') {
          return res.json({ error: { code: -32000, message: 'Incorrect password' } })
        }
        const demo = 'legal winner thank year wave sausage worth useful legal winner thank yellow able cabin dad debris during dose talent layer crater proud drift movie'.split(' ')
        return res.json({ result: { words: demo, word_count: demo.length } })
      }

      case 'update.list-mirrors': {
        globalThis.__mockMirrors ||= [
          // Neutral placeholder — the public demo must not reveal the real release-server address.
          { url: 'https://updates.archipelago.demo/releases/manifest.json', label: 'Origin' },
        ]
        return res.json({ result: { mirrors: globalThis.__mockMirrors } })
      }

      case 'update.get-source': {
        globalThis.__swarmPrefs ||= { source: 'origin', provide_dht: true }
        return res.json({
          result: {
            source: globalThis.__swarmPrefs.source,
            provide_dht: globalThis.__swarmPrefs.provide_dht,
            swarm_available: false, // default build has no iroh-swarm feature
            swarm_enabled: false,
          },
        })
      }

      case 'update.set-source': {
        globalThis.__swarmPrefs ||= { source: 'origin', provide_dht: true }
        if (params && (params.source === 'origin' || params.source === 'swarm')) {
          globalThis.__swarmPrefs.source = params.source
        }
        if (params && typeof params.provide === 'boolean') {
          globalThis.__swarmPrefs.provide_dht = params.provide
        }
        return res.json({
          result: {
            source: globalThis.__swarmPrefs.source,
            provide_dht: globalThis.__swarmPrefs.provide_dht,
            swarm_available: false,
            swarm_enabled: false,
          },
        })
      }

      case 'network.list-requests': {
        return res.json({
          result: {
            requests: [
              { id: 'req-1', from_did: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9', from_name: 'archy-198', type: 'federation-join', status: 'pending', created_at: new Date(Date.now() - 3600000).toISOString() },
            ],
          },
        })
      }

      // ── Monitoring ──────────────────────────────────────────────
      case 'monitoring.current': {
        return res.json({ result: monitoringSnapshot(Math.floor(Date.now() / 1000)) })
      }

      case 'monitoring.history': {
        const count = Math.min(params?.count || 60, 1440)
        const nowSecs = Math.floor(Date.now() / 1000)
        const data = Array.from({ length: count }, (_, i) => monitoringSnapshot(nowSecs - (count - 1 - i) * 60, i))
        return res.json({ result: { resolution: params?.resolution || 'minute', count: data.length, data } })
      }

      case 'monitoring.alerts': {
        const nowSecs = Math.floor(Date.now() / 1000)
        return res.json({
          result: {
            alerts: [
              { id: 'al-1', kind: 'ram_usage', message: 'Memory usage exceeded 85% (LND DB compaction)', value: 87.2, threshold: 85, timestamp: nowSecs - 7200, acknowledged: false },
              { id: 'al-2', kind: 'disk_usage', message: 'Disk usage reached 80% on /var/lib/archipelago', value: 80.4, threshold: 80, timestamp: nowSecs - 86400, acknowledged: true },
              { id: 'al-3', kind: 'backend_error_spike', message: 'RPC latency spiked to 620ms', value: 620, threshold: 500, timestamp: nowSecs - 172800, acknowledged: true },
            ],
          },
        })
      }

      case 'monitoring.alert-rules': {
        return res.json({
          result: {
            rules: [
              { kind: 'disk_usage', enabled: true, threshold: 80, description: 'Disk usage above threshold (%)' },
              { kind: 'ram_usage', enabled: true, threshold: 85, description: 'Memory usage above threshold (%)' },
              { kind: 'container_crash', enabled: true, threshold: 1, description: 'Container restarted unexpectedly' },
              { kind: 'backend_error_spike', enabled: false, threshold: 500, description: 'RPC latency above threshold (ms)' },
              { kind: 'ssl_cert_expiry', enabled: true, threshold: 14, description: 'TLS certificate expires within N days' },
            ],
          },
        })
      }

      case 'monitoring.configure-alert':
      case 'monitoring.acknowledge-alert': {
        return res.json({ result: { success: true } })
      }

      case 'monitoring.export': {
        const count = Math.min(params?.count || 1440, 1440)
        const nowSecs = Math.floor(Date.now() / 1000)
        const data = Array.from({ length: count }, (_, i) => monitoringSnapshot(nowSecs - (count - 1 - i) * 60, i))
        if (params?.format === 'csv') {
          const csv = ['timestamp,cpu_percent,mem_used_bytes,mem_total_bytes,net_rx_bytes,net_tx_bytes,rpc_latency_ms']
            .concat(data.map(s => [s.timestamp, s.system.cpu_percent, s.system.mem_used_bytes, s.system.mem_total_bytes, s.system.net_rx_bytes, s.system.net_tx_bytes, s.rpc_latency_ms].join(',')))
            .join('\n')
          return res.json({ result: { csv, count: data.length } })
        }
        return res.json({ result: { data, count: data.length } })
      }

      case 'monitoring.container-metrics': {
        return res.json({
          result: {
            containers: [
              { name: 'bitcoin-knots', cpu_percent: 8.2, mem_used_bytes: 1_200_000_000, net_rx_bytes: 5_000_000, net_tx_bytes: 3_200_000 },
              { name: 'lnd', cpu_percent: 3.1, mem_used_bytes: 480_000_000, net_rx_bytes: 2_100_000, net_tx_bytes: 1_800_000 },
              { name: 'electrs', cpu_percent: 12.4, mem_used_bytes: 890_000_000, net_rx_bytes: 800_000, net_tx_bytes: 600_000 },
              { name: 'mempool', cpu_percent: 5.6, mem_used_bytes: 320_000_000, net_rx_bytes: 1_500_000, net_tx_bytes: 900_000 },
              { name: 'filebrowser', cpu_percent: 0.8, mem_used_bytes: 45_000_000, net_rx_bytes: 200_000, net_tx_bytes: 150_000 },
            ],
          },
        })
      }

      // ── Fleet / Telemetry ─────────────────────────────────────
      case 'telemetry.fleet-status': {
        return res.json({
          result: {
            nodes: [
              {
                id: 'node-1', name: 'archy-main', did: 'did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2ReMkBe4bR6XBIDNq9',
                status: 'online', last_seen: new Date().toISOString(),
                version: '1.3.0', uptime_secs: 604800,
                system: { cpu_percent: 15.2, mem_used_bytes: 6_200_000_000, mem_total_bytes: 16_000_000_000, disk_used_bytes: 620_000_000_000, disk_total_bytes: 1_800_000_000_000 },
                apps: ['bitcoin-knots', 'lnd', 'electrs', 'mempool', 'filebrowser'],
                tor_connected: true,
              },
              {
                id: 'node-2', name: 'archy-198', did: 'did:key:z6Mkp2z3PJbJHbQ95fGk3CqYVNEPE3VNnFGA7yUYkQjXoZTL',
                status: 'online', last_seen: new Date(Date.now() - 120000).toISOString(),
                version: '1.2.1', uptime_secs: 259200,
                system: { cpu_percent: 8.7, mem_used_bytes: 3_100_000_000, mem_total_bytes: 8_000_000_000, disk_used_bytes: 180_000_000_000, disk_total_bytes: 500_000_000_000 },
                apps: ['bitcoin-knots', 'lnd', 'mempool'],
                tor_connected: true,
              },
              {
                id: 'node-3', name: 'archy-vps', did: 'did:key:z6MknGc3ocHs3zdPiJbnaaqDi5ER7eLYwBqPR4NkDhCUD7Li',
                status: 'offline', last_seen: new Date(Date.now() - 3600000).toISOString(),
                version: '1.3.0', uptime_secs: 0,
                system: { cpu_percent: 0, mem_used_bytes: 0, mem_total_bytes: 4_000_000_000, disk_used_bytes: 45_000_000_000, disk_total_bytes: 80_000_000_000 },
                apps: ['lnd'],
                tor_connected: false,
              },
            ],
          },
        })
      }

      case 'telemetry.fleet-alerts': {
        return res.json({
          result: {
            alerts: [
              { id: 'fa1', node_id: 'node-3', node_name: 'archy-vps', kind: 'node_offline', message: 'Node went offline', timestamp: new Date(Date.now() - 3600000).toISOString(), acknowledged: false },
              { id: 'fa2', node_id: 'node-2', node_name: 'archy-198', kind: 'disk_high', message: 'Disk usage at 36%', timestamp: new Date(Date.now() - 86400000).toISOString(), acknowledged: true },
            ],
          },
        })
      }

      case 'telemetry.fleet-node-history': {
        const nodeId = params?.node_id || 'node-1'
        const points = 60
        const history = []
        for (let i = 0; i < points; i++) {
          history.push({
            timestamp: new Date(Date.now() - (points - i) * 60000).toISOString(),
            cpu_percent: +(8 + Math.random() * 20).toFixed(1),
            mem_used_bytes: 5_000_000_000 + Math.floor(Math.random() * 2_000_000_000),
          })
        }
        return res.json({ result: { node_id: nodeId, history } })
      }

      // ── OpenWRT / TollGate gateway (demo: a thoroughly-used gateway) ──
      case 'openwrt.get-status': {
        return res.json({
          result: {
            host: '192.168.8.1',
            hostname: 'archy-tollgate',
            uptime_secs: 19 * 86400 + 7 * 3600 + 42 * 60,
            release: {
              openwrt_release: 'OpenWrt 24.10.1',
              openwrt_version: 'r28597-0425664679',
              openwrt_board_name: 'glinet,gl-mt3000',
              openwrt_arch: 'aarch64_cortex-a53',
              openwrt_target: 'mediatek/filogic',
            },
            tollgate: {
              installed: true,
              enabled: mockState.tollgateEnabled,
              metric: 'milliseconds',
              step_size_ms: mockState.tollgateStepMs,
              price_per_step: mockState.tollgatePrice,
              min_steps: mockState.tollgateMinSteps,
              currency: 'sat',
              mint_url: mockState.tollgateMint,
            },
            wifi_interfaces: [
              { section: 'default_radio0', ssid: 'TollGate-⚡-2.4G', device: 'radio0', encryption: 'none', network: 'lan', disabled: false },
              { section: 'default_radio1', ssid: 'TollGate-⚡-5G', device: 'radio1', encryption: 'none', network: 'lan', disabled: false },
              { section: 'wifinet2', ssid: 'Archy-Private', device: 'radio1', encryption: 'sae', network: 'lan', disabled: false },
            ],
            wan: {
              configured: true,
              ssid: 'CasaDelSol-5G',
              assoc_ssid: 'CasaDelSol-5G',
              encryption: 'psk2',
              ip: '192.0.2.15',
              internet: true,
              radio0_disabled: false,
              sta_iface: 'wifinet1',
              sta_state: 'COMPLETED',
              wifi_log: 'wlan1: associated -> COMPLETED (RSSI -52 dBm)',
              lan_ip: '172.16.0.1',
              lan_netmask: '255.255.255.0',
              dhcp_start: '100',
              dhcp_limit: '150',
              masq: true,
            },
          },
        })
      }
      case 'openwrt.scan': {
        return res.json({ result: { routers: ['192.168.8.1'] } })
      }
      case 'openwrt.scan-wifi': {
        return res.json({
          result: {
            networks: [
              { ssid: 'CasaDelSol-5G', bssid: 'a4:3e:51:0b:77:21', signal: -52, channel: 44, encryption: 'psk2' },
              { ssid: 'CasaDelSol', bssid: 'a4:3e:51:0b:77:20', signal: -49, channel: 6, encryption: 'psk2' },
              { ssid: 'Vodafone-B221', bssid: '5c:35:3b:9a:12:f0', signal: -71, channel: 1, encryption: 'psk2' },
              { ssid: 'cafe-guest', bssid: '10:27:f5:c4:88:3a', signal: -78, channel: 11, encryption: 'none' },
            ],
          },
        })
      }
      case 'openwrt.provision-tollgate': {
        if (params?.price_per_step != null) mockState.tollgatePrice = params.price_per_step
        if (params?.step_size_ms != null) mockState.tollgateStepMs = params.step_size_ms
        if (params?.min_steps != null) mockState.tollgateMinSteps = params.min_steps
        if (params?.mint_url) mockState.tollgateMint = params.mint_url
        if (params?.enabled != null) mockState.tollgateEnabled = !!params.enabled
        return res.json({ result: { ok: true } })
      }
      case 'openwrt.configure-wan': {
        return res.json({ result: { ok: true } })
      }

      // ── Nostr peer discovery (demo) ──────────────────────────────────
      case 'nostr.discovery-status': {
        return res.json({ result: { enabled: mockState.nostrDiscovery } })
      }
      case 'nostr.set-discovery': {
        mockState.nostrDiscovery = !!params?.enabled
        return res.json({ result: { enabled: mockState.nostrDiscovery } })
      }
      case 'handshake.discover': {
        return res.json({
          result: {
            nodes: [
              { nostr_pubkey: '8f1c9a2b7d4e6f0a3c5b8d1e4f7a0c3b6e9d2f5a8b1c4e7d0a3f6c9b2e5d8a1f', nostr_npub: 'npub13uwf52m04893s79hxc7fmz57jd60jz4kd3nk28h5t2rr3l2xkq0qte0mfz', did: 'did:key:z6MkfV2sQpXm4d8YtR1nWc7uHb3eKj9gLa5xPzD6oTiN8rEw', version: '1.7.99' },
              { nostr_pubkey: '2a4c6e8f0b1d3f5a7c9e1b3d5f7a9c1e3b5d7f9a1c3e5b7d9f1a3c5e7b9d1f3a', nostr_npub: 'npub1z5jvknq0k8f7am7wp0946fmdxu2yjkzr8mfw2n0lqv4tksz3xy8s5c9m2d', did: 'did:key:z6MkrJ8pWx2yNc5vT9qLb4eHu7dKf1gMa3sPzE6oXiQ8nRvw', version: '1.7.99' },
              { nostr_pubkey: '5b7d9f1a3c5e7b9d1f3a5c7e9b1d3f5a7c9e1b3d5f7a9c1e3b5d7f9a1c3e5b7d', nostr_npub: 'npub1t9k2vfmq7x8n3a5wj0c4bz6y1d8s2e7r5m9p0l3q6u4i8o2h7g1f5x9c3vk', did: 'did:key:z6MkhT4wQn8xPc2vL6sRb9eYu3dJf7gKa1mNzD5oWiE8tXvq', version: '1.7.98' },
              { nostr_pubkey: '9c1e3b5d7f9a1c3e5b7d9f1a3c5e7b9d1f3a5c7e9b1d3f5a7c9e1b3d5f7a9c1e', nostr_npub: 'npub1w3e8rkmq2x7n9a4vj5c0bz1y6d3s8e2r7m4p9l0q5u8i3o6h2g7f1x4c9vs', did: 'did:key:z6MkgS9wPm3xNc7vK2sQb5eTu8dHf4gJa6mLzD1oXiR8nEvw', version: '1.7.99' },
            ],
          },
        })
      }
      case 'handshake.connect': {
        const reqId = `preq-${Date.now()}`
        mockState.pendingPeerRequests.push({
          id: reqId,
          from_nostr_pubkey: params?.recipient_nostr_pubkey || 'unknown',
          from_nostr_npub: 'npub1outbounddemo',
          from_did: 'did:key:zOutboundDemo',
          from_name: params?.name || null,
          message: params?.message || null,
          received_at: new Date().toISOString(),
          state: 'pending',
          outbound: true,
        })
        return res.json({ result: { ok: true, sent_to: params?.recipient_nostr_pubkey || '', id: reqId } })
      }
      case 'handshake.poll': {
        return res.json({ result: { polled: 4, new_requests: [], applied_invites: [], rejected_outbound: [], skipped: [] } })
      }
      case 'federation.list-pending-requests': {
        return res.json({ result: { requests: mockState.pendingPeerRequests } })
      }
      case 'federation.approve-request': {
        const r = mockState.pendingPeerRequests.find((x) => x.id === params?.id)
        if (r) r.state = 'approved'
        return res.json({ result: { approved: true, id: params?.id || '' } })
      }
      case 'federation.reject-request': {
        const r = mockState.pendingPeerRequests.find((x) => x.id === params?.id)
        if (r) r.state = 'rejected'
        return res.json({ result: { rejected: true, id: params?.id || '' } })
      }
      // Mirrors federation/handlers.rs handle_federation_cancel_request: only
      // an OUTBOUND request still in 'sent' state may be cancelled, the row is
      // then dropped locally, and `notify` defaults to TRUE (the daemon calls
      // cancelling without notifying a footgun — the peer would keep showing
      // an unanswerable request).
      case 'federation.cancel-request': {
        const id = params?.id
        if (!id) {
          return res.json({ error: { code: -32602, message: 'Missing id' } })
        }
        const notify = params?.notify === undefined ? true : !!params.notify
        const req = mockState.pendingPeerRequests.find((x) => x.id === id)
        if (!req) {
          return res.json({
            error: { code: -32603, message: `Pending request not found: ${id}` },
          })
        }
        if (!req.outbound || req.state !== 'sent') {
          return res.json({
            error: {
              code: -32603,
              message: `Can only cancel outbound requests in Sent state (outbound=${!!req.outbound}, state=${req.state})`,
            },
          })
        }
        mockState.pendingPeerRequests = mockState.pendingPeerRequests.filter((x) => x.id !== id)
        return res.json({ result: { cancelled: true, id, notified: notify } })
      }
      // Mirrors handle_federation_notify_did_change: fans the rotation proof
      // out to every known node and reports per-peer results. The demo has no
      // real peers to reach, so every known node reports ok over its recorded
      // transport — the shape the UI renders, not a fabricated failure.
      case 'federation.notify-did-change': {
        for (const key of ['old_did', 'new_did', 'proof_signature', 'proof_message']) {
          if (!params?.[key]) {
            return res.json({ error: { code: -32602, message: `Missing '${key}'` } })
          }
        }
        const results = sessionFederationNodes().map((n) => ({
          did: n.did,
          status: 'ok',
          transport: n.transport || 'tor',
        }))
        return res.json({
          result: { notified: results.length, failed: 0, results },
        })
      }

      // ── Mesh chat polish + flash flow (v1.7.117/118 demo coverage) ──────

      // Mirrors the real daemon's size-based tier logic
      // (typed_messages.rs handle_mesh_transport_advice) so the demo shows the
      // SAME modals a real node would — the chooser only appears in the narrow
      // fits-both band, never unconditionally. Demo radio is Meshcore; every
      // peer except mountain-node (3) is federation-reachable (Tor + FIPS).
      case 'mesh.transport-advice': {
        const MESH_AUTO_MAX = 1024
        const MESH_HARD_MAX = 2300
        const TOR_LARGE_WARN = 5 * 1024 * 1024
        const LORA_BYTES_PER_SEC = 50
        const size = params?.size || 0
        const federated = params?.contact_id !== 3
        let tier, reason
        if (size <= MESH_AUTO_MAX) {
          tier = 'auto-mesh'; reason = 'Small enough to send inline over mesh'
        } else if (size <= MESH_HARD_MAX) {
          if (federated) { tier = 'choose'; reason = 'Fits over mesh (slow) or Tor (instant)' }
          else { tier = 'auto-mesh'; reason = 'No Tor path — sending inline over mesh' }
        } else if (size <= TOR_LARGE_WARN) {
          if (federated) { tier = 'tor-only'; reason = 'Too large for mesh — Tor only' }
          else { tier = 'impossible'; reason = 'Too large for mesh, and peer has no Tor path' }
        } else {
          if (federated) { tier = 'tor-only'; reason = 'Large file — receiver fetch may be slow' }
          else { tier = 'impossible'; reason = 'Too large, and peer has no Tor path' }
        }
        return res.json({
          result: {
            tier,
            est_seconds: Math.max(1, Math.ceil(size / LORA_BYTES_PER_SEC)),
            has_tor: federated,
            has_fips: federated,
            last_transport: federated ? 'fips' : null,
            reason,
            size,
            mesh_auto_max: MESH_AUTO_MAX,
            mesh_hard_max: MESH_HARD_MAX,
          },
        })
      }

      // Attachment sends — same RPC surface a real node exposes (dispatch.rs),
      // so the demo attach flow exercises the identical frontend code path.
      case 'mesh.send-content-inline': {
        const contactId = params?.contact_id
        const peerNames = { 1: 'archy-198', 2: 'satoshi-relay', 3: 'mountain-node', 4: 'bunker-alpha' }
        const bytes = Buffer.from(params?.bytes_b64 || '', 'base64')
        const cid = 'demo-inline-' + crypto.createHash('sha256').update(bytes).digest('hex').slice(0, 16)
        const meshStore = currentStore().mesh
        meshStore.blobs[cid] = {
          mime: params?.mime || 'application/octet-stream',
          filename: params?.filename || null,
          b64: params?.bytes_b64 || '',
        }
        const id = 100 + meshStore.dynamic.length
        meshStore.dynamic.push({
          id, direction: 'sent', peer_contact_id: contactId, peer_name: peerNames[contactId] || `peer-${contactId}`,
          plaintext: `📎 ${params?.filename || params?.mime || 'attachment'}`,
          timestamp: new Date().toISOString(), delivered: true, encrypted: contactId !== 3,
          transport: 'meshcore',
          sender_pubkey: 'demo02abababababababababababababababababababababababababababab',
          sender_seq: id, message_type: 'content_ref',
          typed_payload: {
            cid, inline: true,
            mime: params?.mime || 'application/octet-stream',
            filename: params?.filename || null,
            caption: params?.caption || null,
            size: bytes.length,
            sender_onion: '', cap_token: 'demo', cap_exp: Math.floor(Date.now() / 1000) + 86400,
          },
        })
        return res.json({ result: { sent: true, message_id: id, cid, size: bytes.length } })
      }

      case 'mesh.send-content': {
        const contactId = params?.contact_id
        const peerNames = { 1: 'archy-198', 2: 'satoshi-relay', 3: 'mountain-node', 4: 'bunker-alpha' }
        const meshStore = currentStore().mesh
        const blob = meshStore.blobs[params?.cid] || {}
        const size = blob.b64 ? Buffer.from(blob.b64, 'base64').length : 0
        const id = 100 + meshStore.dynamic.length
        meshStore.dynamic.push({
          id, direction: 'sent', peer_contact_id: contactId, peer_name: peerNames[contactId] || `peer-${contactId}`,
          plaintext: `📎 ${blob.filename || blob.mime || 'attachment'}`,
          timestamp: new Date().toISOString(), delivered: true, encrypted: contactId !== 3,
          transport: 'tor',
          sender_pubkey: 'demo02abababababababababababababababababababababababababababab',
          sender_seq: id, message_type: 'content_ref',
          typed_payload: {
            cid: params?.cid, inline: false,
            mime: blob.mime || 'application/octet-stream',
            filename: blob.filename || null,
            caption: params?.caption || null,
            size,
            thumb_bytes: blob.thumb_b64 || undefined,
            sender_onion: '', cap_token: 'demo', cap_exp: Math.floor(Date.now() / 1000) + 86400,
          },
        })
        return res.json({ result: { sent: true, message_id: id, cid: params?.cid, size } })
      }

      case 'mesh.fetch-content': {
        const blob = currentStore().mesh.blobs[params?.cid]
        if (!blob) return res.json({ error: { code: -32000, message: 'Content not found' } })
        return res.json({
          result: { local_url: `data:${blob.mime || 'application/octet-stream'};base64,${blob.b64}` },
        })
      }

      case 'mesh.flash-list-firmware': {
        const versions = {
          meshcore: ['v1.7.4', 'v1.7.2', 'v1.6.9'],
          meshtastic: ['2.5.15', '2.5.13', '2.4.2'],
          reticulum: ['latest (rnodeconf autoinstall)'],
        }
        return res.json({ result: { versions: versions[params?.family] || [] } })
      }
      case 'mesh.flash-device': {
        mockState.flashJob = {
          startedAt: Date.now(),
          board: params?.board || 'heltec-v3',
          family: params?.family || 'meshcore',
          path: params?.path || '/dev/ttyUSB0',
          cancelled: false,
        }
        return res.json({ result: { started: true } })
      }
      case 'mesh.flash-status': {
        const job = mockState.flashJob
        if (!job) return res.json({ result: { active: false, done: true, log_tail: [] } })
        const t = (Date.now() - job.startedAt) / 1000
        // Scripted 35s job: download → erase → write → done.
        let stage = 'downloading'
        let percent = Math.min(100, Math.round((t / 8) * 100))
        if (job.cancelled) stage = 'failed'
        else if (t > 30) { stage = 'done'; percent = 100 }
        else if (t > 14) { stage = 'writing'; percent = Math.min(100, Math.round(((t - 14) / 16) * 100)) }
        else if (t > 8) { stage = 'erasing'; percent = null }
        const log = [
          `Fetching ${job.family} firmware for ${job.board}...`,
          ...(t > 4 ? ['Downloaded 2.1 MB firmware image'] : []),
          ...(t > 8 ? ['esptool erase_flash: chip erase completed'] : []),
          ...(t > 14 ? [`Writing at 0x00010000... (${percent || 0} %)`] : []),
          ...(stage === 'done' ? ['Hash of data verified.', 'Hard resetting via RTS pin...'] : []),
          ...(job.cancelled ? ['ERROR: flash cancelled by user'] : []),
        ]
        const active = stage !== 'done' && stage !== 'failed'
        if (!active) mockState.flashJob = active ? job : job // keep for final poll
        return res.json({
          result: {
            active,
            board: job.board,
            family: job.family,
            path: job.path,
            stage,
            percent,
            log_tail: log,
            done: !active,
            error: job.cancelled ? 'Flash cancelled' : null,
          },
        })
      }
      case 'mesh.flash-cancel': {
        if (mockState.flashJob) mockState.flashJob.cancelled = true
        return res.json({ result: { cancelled: true } })
      }

      // ── Contacts (peer aliasing) ────────────────────────────────────────
      // Mirrors typed_messages.rs handle_mesh_contacts_list /
      // handle_mesh_contacts_save: contact entries are keyed by pubkey_hex and
      // merged OVER the peer list, so a peer with no entry still appears with
      // null alias/notes. Peers without a pubkey_hex are omitted, exactly as
      // the daemon omits them (it only pushes when peer.pubkey_hex is Some).
      case 'mesh.contacts-list': {
        const contacts = currentStore().mesh.contacts
        const out = DEMO_MESH_PEERS.filter((p) => p.pubkey_hex).map((p) => {
          const entry = contacts[p.pubkey_hex] || {}
          return {
            pubkey: p.pubkey_hex,
            contact_id: p.contact_id,
            name: p.advert_name,
            alias: entry.alias ?? null,
            notes: entry.notes ?? null,
            pinned: entry.pinned ?? false,
            blocked: entry.blocked ?? false,
          }
        })
        return res.json({ result: { contacts: out } })
      }

      // Upsert semantics match the daemon's: a field absent from params leaves
      // the stored value alone (it only assigns when the key is present), so
      // saving an alias never clears existing notes.
      case 'mesh.contacts-save': {
        const pubkey = params?.pubkey
        if (!pubkey) {
          return res.json({ error: { code: -32602, message: 'Missing pubkey' } })
        }
        const contacts = currentStore().mesh.contacts
        const entry = contacts[pubkey] || { alias: null, notes: null, pinned: false, blocked: false }
        if (params.alias !== undefined) entry.alias = params.alias
        if (params.notes !== undefined) entry.notes = params.notes
        if (params.pinned !== undefined) entry.pinned = !!params.pinned
        contacts[pubkey] = entry
        return res.json({
          result: {
            saved: true,
            pubkey,
            alias: entry.alias ?? null,
            notes: entry.notes ?? null,
            pinned: entry.pinned ?? false,
            blocked: entry.blocked ?? false,
          },
        })
      }

      // Mirrors status.rs handle_mesh_clear_all, which deletes the mesh state
      // files and clears in-memory state, returning { status: "cleared" }.
      // Here that means dropping this session's sent messages and blobs.
      case 'mesh.clear-all': {
        const meshStore = currentStore().mesh
        meshStore.dynamic.length = 0
        meshStore.blobs = {}
        return res.json({ result: { status: 'cleared' } })
      }

      // ── Scheduled messages ──────────────────────────────────────────────
      // Mirrors assistant.rs handle_mesh_schedule_message / list / cancel and
      // scheduler.rs ScheduledMessage { id, contact_id, channel, body,
      // fire_at, attempts }. The daemon requires body + fire_at and one of
      // contact_id / channel, and errors otherwise — so does this.
      case 'mesh.schedule-message': {
        const body = params?.body
        const fireAt = params?.fire_at
        if (typeof body !== 'string') {
          return res.json({ error: { code: -32602, message: 'body is required' } })
        }
        if (typeof fireAt !== 'number') {
          return res.json({ error: { code: -32602, message: 'fire_at (unix seconds) is required' } })
        }
        const contactId = params?.contact_id
        const channel = params?.channel
        if (contactId === undefined && channel === undefined) {
          return res.json({
            error: { code: -32602, message: 'either contact_id or channel is required' },
          })
        }
        const meshStore = currentStore().mesh
        const entry = {
          id: meshStore.nextScheduledId++,
          contact_id: contactId ?? null,
          channel: channel ?? null,
          body,
          fire_at: fireAt,
          attempts: 0,
        }
        meshStore.scheduled.push(entry)
        return res.json({ result: entry })
      }

      // Sorted by fire time, as the daemon's scheduler.list() returns them.
      case 'mesh.list-scheduled': {
        const messages = [...currentStore().mesh.scheduled].sort((a, b) => a.fire_at - b.fire_at)
        return res.json({ result: { messages } })
      }

      case 'mesh.cancel-scheduled': {
        const id = params?.id
        if (typeof id !== 'number') {
          return res.json({ error: { code: -32602, message: 'id is required' } })
        }
        const meshStore = currentStore().mesh
        const before = meshStore.scheduled.length
        meshStore.scheduled = meshStore.scheduled.filter((m) => m.id !== id)
        return res.json({ result: { cancelled: meshStore.scheduled.length < before } })
      }

      // ── Mesh assistant ──────────────────────────────────────────────────
      // Mirrors assistant.rs handle_mesh_assistant_status: the demo reports no
      // Ollama and no Claude key, which is the honest answer for a browser
      // demo with no local model — the UI's "not detected" path is what a
      // visitor should see, not a fabricated model list.
      case 'mesh.assistant-status': {
        const cfg = mockState.assistantConfig
        return res.json({
          result: {
            enabled: cfg.enabled,
            model: cfg.model,
            trusted_only: cfg.trusted_only,
            backend: cfg.backend,
            allowed_contacts: cfg.allowed_contacts,
            default_model: 'llama3.2:3b',
            ollama_detected: false,
            claude_available: false,
            models: [],
            denied_askers: [],
          },
        })
      }

      // Mirrors handle_mesh_assistant_configure's key-presence semantics:
      // model present+string sets, present+null clears to default, absent
      // leaves; allowed_contacts present+array replaces the allowlist.
      case 'mesh.assistant-configure': {
        const cfg = mockState.assistantConfig
        if (params?.enabled !== undefined) cfg.enabled = !!params.enabled
        if (params?.trusted_only !== undefined) cfg.trusted_only = !!params.trusted_only
        if (params?.backend !== undefined) cfg.backend = params.backend
        if (params && 'model' in params) cfg.model = params.model
        if (Array.isArray(params?.allowed_contacts)) cfg.allowed_contacts = params.allowed_contacts
        return res.json({
          result: {
            enabled: cfg.enabled,
            model: cfg.model,
            trusted_only: cfg.trusted_only,
            backend: cfg.backend,
            allowed_contacts: cfg.allowed_contacts,
          },
        })
      }

      // ── Chat mutations ──────────────────────────────────────────────────
      // Reactions, replies, receipts and forwards are new typed messages
      // carrying a target key (typed_messages.rs send_typed_wire); edits and
      // deletes mutate the target in place (mesh/mod.rs apply_local_edit /
      // apply_local_delete). The UI derives reaction chips and reply quotes
      // from exactly these shapes (Mesh.vue reactionIndex /
      // replyTargetPreview), so a bare ack renders as nothing at all.
      case 'mesh.send-reaction': {
        const emoji = params?.emoji ?? ''
        pushMeshMessage({
          contact_id: params?.contact_id ?? null,
          plaintext: emoji,
          message_type: 'reaction',
          typed_payload: {
            target: { sender_pubkey: params?.target_pubkey, sender_seq: params?.target_seq },
            // An empty emoji clears this reactor's reaction, per the UI's
            // reactionIndex (freshest per reactor wins; '' deletes).
            emoji,
          },
        })
        return res.json({ result: { ok: true, sent: true } })
      }

      case 'mesh.send-reply': {
        const text = params?.text ?? ''
        pushMeshMessage({
          contact_id: params?.contact_id ?? null,
          plaintext: text,
          message_type: 'reply',
          typed_payload: {
            target: { sender_pubkey: params?.target_pubkey, sender_seq: params?.target_seq },
            text,
          },
        })
        return res.json({ result: { ok: true, sent: true } })
      }

      case 'mesh.send-read-receipt': {
        pushMeshMessage({
          contact_id: params?.contact_id ?? null,
          plaintext: '',
          message_type: 'read_receipt',
          typed_payload: {
            target: { sender_pubkey: params?.target_pubkey, sender_seq: params?.target_seq },
          },
        })
        return res.json({ result: { ok: true, sent: true } })
      }

      // apply_local_edit: replaces plaintext and merges { edited_at, text }
      // into the existing typed_payload of the own-Sent message with that seq.
      case 'mesh.edit-message': {
        const seq = params?.target_seq
        const newText = params?.new_text ?? ''
        if (seq == null) {
          return res.json({ error: { code: -32602, message: 'target_seq is required' } })
        }
        const overrides = currentStore().mesh.overrides
        const prior = overrides[seq] || {}
        overrides[seq] = {
          ...prior,
          plaintext: newText,
          typed_payload: {
            ...(prior.typed_payload || {}),
            edited_at: Math.floor(Date.now() / 1000),
            text: newText,
          },
        }
        return res.json({ result: { ok: true, sent: true } })
      }

      // apply_local_delete: tombstone in place — the daemon does NOT remove
      // the row, it rewrites plaintext, sets typed_payload { deleted: true }
      // and message_type 'delete'. Mirrored exactly; do not "simplify" this
      // into a splice, the UI renders the tombstone bubble.
      case 'mesh.delete-message': {
        const seq = params?.target_seq
        if (seq == null) {
          return res.json({ error: { code: -32602, message: 'target_seq is required' } })
        }
        currentStore().mesh.overrides[seq] = {
          plaintext: '🗑 message deleted',
          typed_payload: { deleted: true },
          message_type: 'delete',
        }
        return res.json({ result: { ok: true, sent: true } })
      }

      case 'mesh.forward-message': {
        const sourceId = params?.source_message_id
        const all = applyMeshOverrides(currentStore().mesh.dynamic)
        const source = all.find((m) => m.id === sourceId)
        pushMeshMessage({
          contact_id: params?.contact_id ?? null,
          plaintext: source ? source.plaintext : '(forwarded)',
          message_type: source?.message_type === 'content_ref' ? 'content_ref' : 'text',
          typed_payload: source?.typed_payload ?? null,
        })
        return res.json({ result: { ok: true, sent: true } })
      }

      case 'mesh.send-channel': {
        pushMeshMessage({
          contact_id: null,
          channel: params?.channel ?? 0,
          plaintext: params?.message ?? '',
          message_type: 'text',
        })
        return res.json({ result: { ok: true, sent: true } })
      }

      // mesh.refresh and mesh.reboot-radio stay bare acknowledgements ON
      // PURPOSE: the daemon's handlers have no message-store effect either
      // (refresh re-polls the radio, reboot-radio power-cycles it), so giving
      // them demo-side state would be divergence, not parity. Do not "fix".
      case 'mesh.refresh':
      case 'mesh.reboot-radio': {
        return res.json({ result: { ok: true, sent: true } })
      }

      default: {
        console.log(`[RPC] Unknown method: ${method}`)
        return res.json({
          error: {
            code: -32601,
            message: `Method not found: ${method}`,
          },
        })
      }
    }
  } catch (error) {
    console.error('[RPC Error]', error)
    return res.json({
      error: {
        code: -32603,
        message: error.message,
      },
    })
  }
})

// =============================================================================
// Mock FileBrowser API (for Cloud page in demo/Docker deployments)
// =============================================================================
const SEED_FILES = {
  '/': [
    { name: 'Music', path: '/Music', size: 0, modified: '2025-03-01T10:00:00Z', isDir: true, type: '' },
    { name: 'Documents', path: '/Documents', size: 0, modified: '2025-02-28T14:30:00Z', isDir: true, type: '' },
    { name: 'Photos', path: '/Photos', size: 0, modified: '2025-02-20T09:15:00Z', isDir: true, type: '' },
  ],
  '/Music': [
    { name: 'Bad Actors Reveal.mp3', path: '/Music/Bad Actors Reveal.mp3', size: 8_400_000, modified: '2025-01-10T12:00:00Z', isDir: false, type: 'audio' },
    { name: 'Architects of Tomorrow.wav', path: '/Music/Architects of Tomorrow.wav', size: 42_000_000, modified: '2025-01-08T15:00:00Z', isDir: false, type: 'audio' },
    { name: 'Sats or Shackles.mp3', path: '/Music/Sats or Shackles.mp3', size: 6_200_000, modified: '2024-12-20T10:00:00Z', isDir: false, type: 'audio' },
    { name: 'The Four Horseman of Technocracy.mp3', path: '/Music/The Four Horseman of Technocracy.mp3', size: 7_800_000, modified: '2024-12-15T11:00:00Z', isDir: false, type: 'audio' },
    { name: 'Inverse Dylan (Remix).mp3', path: '/Music/Inverse Dylan (Remix).mp3', size: 5_600_000, modified: '2024-12-10T16:00:00Z', isDir: false, type: 'audio' },
    { name: 'Hootcoiner.mp3', path: '/Music/Hootcoiner.mp3', size: 4_200_000, modified: '2024-11-28T09:00:00Z', isDir: false, type: 'audio' },
    { name: 'decentrealisation.mp3', path: '/Music/decentrealisation.mp3', size: 5_100_000, modified: '2024-11-20T14:00:00Z', isDir: false, type: 'audio' },
    { name: 'neo-morality.mp3', path: '/Music/neo-morality.mp3', size: 6_800_000, modified: '2024-11-15T11:00:00Z', isDir: false, type: 'audio' },
    { name: 'death is a gift.mp3', path: '/Music/death is a gift.mp3', size: 4_500_000, modified: '2024-11-10T08:00:00Z', isDir: false, type: 'audio' },
    { name: 'Wash the fucking dishes.mp3', path: '/Music/Wash the fucking dishes.mp3', size: 3_900_000, modified: '2024-11-05T13:00:00Z', isDir: false, type: 'audio' },
    { name: 'All the leaves are brown.mp3', path: '/Music/All the leaves are brown.mp3', size: 5_300_000, modified: '2024-10-28T10:00:00Z', isDir: false, type: 'audio' },
    { name: 'Builders not talkers.mp3', path: '/Music/Builders not talkers.mp3', size: 4_700_000, modified: '2024-10-20T15:00:00Z', isDir: false, type: 'audio' },
    { name: 'SMRI.mp3', path: '/Music/SMRI.mp3', size: 5_900_000, modified: '2024-10-15T12:00:00Z', isDir: false, type: 'audio' },
    { name: 'Shadrap.mp3', path: '/Music/Shadrap.mp3', size: 3_400_000, modified: '2024-10-10T09:00:00Z', isDir: false, type: 'audio' },
    { name: 'The Wehrman.mp3', path: '/Music/The Wehrman.mp3', size: 6_100_000, modified: '2024-10-05T14:00:00Z', isDir: false, type: 'audio' },
    { name: 'An Exploited Substrate.mp3', path: '/Music/An Exploited Substrate.mp3', size: 4_800_000, modified: '2024-09-28T11:00:00Z', isDir: false, type: 'audio' },
    { name: 'Govcucks.wav', path: '/Music/Govcucks.wav', size: 38_000_000, modified: '2024-09-20T16:00:00Z', isDir: false, type: 'audio' },
  ],
  '/Documents': [
    { name: 'bitcoin-whitepaper-notes.md', path: '/Documents/bitcoin-whitepaper-notes.md', size: 820, modified: '2025-02-28T14:30:00Z', isDir: false, type: 'text' },
    { name: 'node-setup-checklist.md', path: '/Documents/node-setup-checklist.md', size: 950, modified: '2025-02-25T10:00:00Z', isDir: false, type: 'text' },
    { name: 'lightning-channels.csv', path: '/Documents/lightning-channels.csv', size: 680, modified: '2025-02-20T16:00:00Z', isDir: false, type: 'text' },
    { name: 'sovereignty-manifesto.txt', path: '/Documents/sovereignty-manifesto.txt', size: 1100, modified: '2025-02-15T12:00:00Z', isDir: false, type: 'text' },
    { name: 'backup-log.json', path: '/Documents/backup-log.json', size: 1450, modified: '2025-03-01T02:00:00Z', isDir: false, type: 'text' },
  ],
  '/Photos': [
    { name: 'node-rack-setup.jpg', path: '/Photos/node-rack-setup.jpg', size: 2_400_000, modified: '2025-02-20T09:15:00Z', isDir: false, type: 'image' },
    { name: 'bitcoin-conference-2024.jpg', path: '/Photos/bitcoin-conference-2024.jpg', size: 3_100_000, modified: '2024-12-15T14:30:00Z', isDir: false, type: 'image' },
    { name: 'lightning-network-visualization.png', path: '/Photos/lightning-network-visualization.png', size: 1_800_000, modified: '2025-01-10T11:00:00Z', isDir: false, type: 'image' },
    { name: 'home-server-build.jpg', path: '/Photos/home-server-build.jpg', size: 2_900_000, modified: '2024-11-20T16:45:00Z', isDir: false, type: 'image' },
    { name: 'sunset-from-balcony.jpg', path: '/Photos/sunset-from-balcony.jpg', size: 4_200_000, modified: '2025-02-14T18:30:00Z', isDir: false, type: 'image' },
  ],
  // NOTE: no seeded Videos folder — every seeded file must be backed by real,
  // playable content (curated videos can still be added via demo/files/Videos).
}

const SEED_FILE_CONTENTS = {
  '/Documents/bitcoin-whitepaper-notes.md': `# Bitcoin Whitepaper Notes\n\n## Key Concepts\n\n### Peer-to-Peer Electronic Cash\n- No trusted third party needed\n- Double-spending solved via proof-of-work\n- Longest chain = truth\n\n### Proof of Work\n- SHA-256 based hashing\n- Difficulty adjusts every 2016 blocks (~2 weeks)\n- Incentive: block reward + transaction fees\n\n## My Thoughts\n- The 21M supply cap is genius - digital scarcity\n- Lightning Network solves the scaling concern\n- Self-custody is the whole point`,
  '/Documents/node-setup-checklist.md': `# Archipelago Node Setup Checklist\n\n## Hardware\n- [x] Intel NUC / Mini PC (16GB RAM minimum)\n- [x] 2TB NVMe SSD\n- [x] USB drive for installer\n- [x] Ethernet cable\n\n## Core Apps\n- [x] Bitcoin Knots\n- [x] LND\n- [x] Mempool Explorer\n- [ ] BTCPay Server\n- [ ] Fedimint`,
  '/Documents/lightning-channels.csv': `channel_id,peer_alias,capacity_sats,local_balance,remote_balance,status\nch_001,ACINQ,5000000,2450000,2550000,active\nch_002,WalletOfSatoshi,2000000,1200000,800000,active\nch_003,Voltage,10000000,4500000,5500000,active\nch_004,Kraken,3000000,1800000,1200000,active`,
  '/Documents/sovereignty-manifesto.txt': `THE SOVEREIGNTY MANIFESTO\n=========================\n\nWe hold these truths to be self-evident:\n\n1. Your data belongs to you.\n2. Your money should be uncensorable.\n3. Your communications should be private.\n4. Your compute should be sovereign.\n5. Your identity should be self-issued.\n\nRun your own node. Hold your own keys. Own your own data. Be sovereign.`,
  '/Documents/backup-log.json': JSON.stringify({ backups: [{ id: 'bkp-2025-03-01', timestamp: '2025-03-01T02:00:00Z', type: 'full', apps: ['bitcoin-knots', 'lnd', 'mempool'], size_mb: 2340, status: 'success' }] }, null, 2),
}

// Curated real files: drop files into <repo>/demo/files/<Folder>/<file> (or the
// committed library in <repo>/demo/content/) and they replace the seeded cloud
// content for every visitor (read-only — visitors can view/download/buy them but
// only maintainers add them, via git = the "private login"). If both are
// absent/empty the hardcoded seeds above are kept. Binary files are streamed
// from disk on demand (diskFilePaths); text is inlined.
const diskFilePaths = {}
function loadDemoDiskFiles() {
  // demo/files (drop-in overrides) wins over demo/content (committed library)
  // when both provide the same top-level folder.
  const roots = [
    path.join(__dirname, '..', 'demo', 'files'),
    path.join(__dirname, '..', 'demo', 'content'),
  ]
  const tree = { '/': [] }
  const contents = {}
  const TEXT_EXT = new Set(['txt', 'md', 'json', 'csv', 'log', 'yaml', 'yml', 'xml', 'conf', 'ini'])
  const addFile = (abs, relDir, name) => {
    let st; try { st = fsSync.statSync(abs) } catch { return }
    const ext = (name.includes('.') ? name.split('.').pop() : '').toLowerCase()
    const type = fbType(name)
    const rel = relDir === '/' ? `/${name}` : `${relDir}/${name}`
    tree[relDir].push({ name, path: rel, size: st.size, modified: st.mtime.toISOString(), isDir: false, type })
    if (TEXT_EXT.has(ext) && st.size < 1_000_000) {
      try { contents[rel] = fsSync.readFileSync(abs, 'utf-8') } catch { /* skip */ }
    } else {
      diskFilePaths[rel] = abs // streamed from disk by the raw handler
    }
  }
  const walk = (absDir, relDir) => {
    let entries
    try { entries = fsSync.readdirSync(absDir, { withFileTypes: true }) } catch { return }
    tree[relDir] = tree[relDir] || []
    for (const e of entries) {
      if (e.name.startsWith('.')) continue
      const abs = path.join(absDir, e.name)
      if (e.isDirectory()) {
        const rel = relDir === '/' ? `/${e.name}` : `${relDir}/${e.name}`
        tree[relDir].push({ name: e.name, path: rel, size: 0, modified: new Date().toISOString(), isDir: true, type: '' })
        walk(abs, rel)
      } else {
        addFile(abs, relDir, e.name)
      }
    }
  }
  const claimed = new Set()
  for (const root of roots) {
    let top
    try { top = fsSync.readdirSync(root, { withFileTypes: true }) } catch { continue }
    for (const e of top) {
      if (e.name.startsWith('.')) continue
      if (e.isDirectory()) {
        // Normalize top-level folder names for display/merge (music → Music).
        const display = e.name.charAt(0).toUpperCase() + e.name.slice(1)
        if (claimed.has(display)) continue
        claimed.add(display)
        const rel = `/${display}`
        tree['/'].push({ name: display, path: rel, size: 0, modified: new Date().toISOString(), isDir: true, type: '' })
        tree[rel] = tree[rel] || []
        walk(path.join(root, e.name), rel)
      } else if (!tree['/'].some(x => x.name === e.name)) {
        addFile(path.join(root, e.name), '/', e.name)
      }
    }
  }
  if (!tree['/'].length) return // nothing curated → keep the hardcoded seeds
  // Per-folder MERGE: a top-level folder present in demo/files replaces that
  // seed folder; seed folders the curator didn't provide (e.g. sample Documents)
  // are kept. So dropping real Music/ doesn't wipe the sample Documents.
  const provided = new Set(tree['/'].map(e => e.name))
  for (const k of Object.keys(SEED_FILES)) {
    if (k !== '/' && provided.has(k.split('/')[1])) delete SEED_FILES[k]
  }
  for (const k of Object.keys(SEED_FILE_CONTENTS)) {
    if (provided.has(k.split('/')[1])) delete SEED_FILE_CONTENTS[k]
  }
  const keptRoot = (SEED_FILES['/'] || []).filter(e => !provided.has(e.name))
  for (const [k, v] of Object.entries(tree)) { if (k !== '/') SEED_FILES[k] = v }
  SEED_FILES['/'] = [...keptRoot, ...tree['/']]
  Object.assign(SEED_FILE_CONTENTS, contents)
  console.log(`[Demo] Merged curated files from demo/files (${Object.keys(diskFilePaths).length} binary, ${Object.keys(contents).length} text; folders: ${[...provided].join(', ')})`)
}
loadDemoDiskFiles()

// FileBrowser UI (demo placeholder when launched directly)
app.get('/app/filebrowser/', (req, res) => {
  res.type('html').send(`<!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>File Browser</title><style>*{margin:0;padding:0;box-sizing:border-box}body{background:#1a1a2e;color:#e0e0e0;font-family:system-ui,sans-serif;display:flex;align-items:center;justify-content:center;min-height:100vh}
.card{background:rgba(0,0,0,0.4);border:1px solid rgba(255,255,255,0.1);border-radius:16px;padding:48px;text-align:center;max-width:400px;backdrop-filter:blur(20px)}
h1{font-size:24px;margin-bottom:12px}p{color:rgba(255,255,255,0.6);font-size:14px;line-height:1.6}</style></head>
<body><div class="card"><h1>File Browser</h1><p>File Browser is running. Use the Cloud page in Archipelago to manage your files.</p></div></body></html>`)
})

// FileBrowser login - return mock JWT
app.post('/app/filebrowser/api/login', (req, res) => {
  res.send('"mock-filebrowser-token-demo"')
})

// ── Per-session file store helpers ──────────────────────────────────────────
// store().files = { tree: { '<dir>': [entries] }, contents: { '<path>': string|Buffer }, bytes }
const FB_QUOTA_BYTES = Number(process.env.DEMO_FILE_QUOTA_BYTES) || 50 * 1024 * 1024

function fbNormalize(raw) {
  // → leading slash, no trailing slash (root stays '/')
  // Express already decodes path parameters. Preserve literal %, + and ? names.
  const p = '/' + (raw || '').split('/').filter(Boolean).join('/')
  return p === '/' ? '/' : p.replace(/\/+$/, '')
}
function fbParent(p) {
  const i = p.lastIndexOf('/')
  return i <= 0 ? '/' : p.slice(0, i)
}
function fbBase(p) { return p.slice(p.lastIndexOf('/') + 1) }
function fbType(name) {
  const ext = (name.includes('.') ? name.split('.').pop() : '').toLowerCase()
  if (['mp3', 'wav', 'flac', 'ogg', 'm4a', 'aac'].includes(ext)) return 'audio'
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp'].includes(ext)) return 'image'
  if (['mp4', 'webm', 'mov', 'mkv', 'avi'].includes(ext)) return 'video'
  if (['txt', 'md', 'json', 'csv', 'log', 'yaml', 'yml', 'xml', 'conf', 'ini'].includes(ext)) return 'text'
  return ''
}
function fbContentType(name) {
  const t = fbType(name)
  const ext = (name.includes('.') ? name.split('.').pop() : '').toLowerCase()
  if (t === 'audio') return ext === 'wav' ? 'audio/wav' : 'audio/mpeg'
  if (t === 'image') return ext === 'png' ? 'image/png' : ext === 'svg' ? 'image/svg+xml' : 'image/jpeg'
  if (t === 'video') return 'video/mp4'
  return 'text/plain; charset=utf-8'
}
function fbListResponse(res, items) {
  res.json({
    items,
    numDirs: items.filter(i => i.isDir).length,
    numFiles: items.filter(i => !i.isDir).length,
    sorting: { by: 'name', asc: true },
  })
}

installDemoUploads(app, { currentStore, quota: FB_QUOTA_BYTES, normalize: fbNormalize, parent: fbParent, base: fbBase, type: fbType })

// FileBrowser list resources (root: /api/resources or /api/resources/)
app.get(['/app/filebrowser/api/resources', '/app/filebrowser/api/resources/*'], (req, res) => {
  const dir = fbNormalize(req.params[0] || '')
  const { tree, contents } = currentStore().files
  if (tree[dir]) return fbListResponse(res, tree[dir])
  const entry = (tree[fbParent(dir)] || []).find(e => e.path === dir)
  if (!entry) return res.sendStatus(404)
  const checksums = req.query.checksum === 'sha256' && contents[dir] !== undefined
    ? { sha256: crypto.createHash('sha256').update(contents[dir]).digest('hex') } : undefined
  res.json({ ...entry, checksums })
})

// FileBrowser POST = upload a file OR create a folder (trailing slash ⇒ folder)
app.post('/app/filebrowser/api/resources/*', (req, res) => {
  const store = currentStore()
  const { tree, contents } = store.files
  const isFolder = (req.params[0] || '').endsWith('/')
  const full = fbNormalize(req.params[0] || '')
  const parent = fbParent(full)
  const name = fbBase(full)
  if (!name) return res.sendStatus(400)
  if (!tree[parent]) tree[parent] = []

  if (isFolder) {
    if (!tree[parent].some(e => e.name === name && e.isDir)) {
      tree[parent].push({ name, path: full, size: 0, modified: new Date().toISOString(), isDir: true, type: '' })
      if (!tree[full]) tree[full] = []
    }
    return res.sendStatus(200)
  }

  // Reserve incoming bytes immediately, including concurrent legacy/TUS writes.
  const chunks = []
  let size = 0
  let aborted = false
  const release = () => { store.files.reserved -= size; size = 0 }
  req.on('data', (c) => {
    if (aborted) return
    if (store.files.bytes + store.files.reserved + c.length > FB_QUOTA_BYTES) {
      aborted = true
      release()
      res.status(507).send('Demo storage quota exceeded')
      return
    }
    size += c.length
    store.files.reserved += c.length
    chunks.push(c)
  })
  req.on('end', () => {
    if (aborted) return
    const buf = Buffer.concat(chunks)
    release()
    if (!tree[parent] || store.files.uploads.has(full) || tree[parent].some(e => e.name === name && e.isDir)) return res.sendStatus(409)
    if (store.files.uploaded.has(full)) store.files.bytes -= Buffer.byteLength(contents[full])
    tree[parent] = tree[parent].filter(e => e.name !== name)
    tree[parent].push({ name, path: full, size: buf.length, modified: new Date().toISOString(), isDir: false, type: fbType(name) })
    contents[full] = buf
    store.files.bytes += buf.length
    store.files.uploaded.add(full)
    res.sendStatus(200)
  })
  req.on('aborted', () => { aborted = true; release() })
  req.on('error', () => { aborted = true; release(); if (!res.headersSent) res.sendStatus(400) })
})

// FileBrowser delete (file or folder + its subtree)
app.delete('/app/filebrowser/api/resources/*', (req, res) => {
  const store = currentStore()
  const { tree, contents } = store.files
  const full = fbNormalize(req.params[0] || '')
  const parent = fbParent(full)
  if (tree[parent]) {
    if (store.files.uploaded.delete(full)) store.files.bytes -= Buffer.byteLength(contents[full])
    tree[parent] = tree[parent].filter(e => e.path !== full)
  }
  // Cancel unfinished sessions in the deleted subtree too.
  for (const [name, upload] of store.files.uploads) {
    if (name === full || name.startsWith(full + '/')) {
      store.files.reserved -= upload.length
      store.files.uploads.delete(name)
    }
  }
  // Recursively drop a directory's children.
  if (tree[full]) {
    const stack = [full]
    while (stack.length) {
      const d = stack.pop()
      for (const e of tree[d] || []) {
        if (e.isDir) stack.push(e.path)
        else {
          if (store.files.uploaded.delete(e.path)) store.files.bytes -= Buffer.byteLength(contents[e.path])
          delete contents[e.path]
        }
      }
      delete tree[d]
    }
  }
  delete contents[full]
  res.sendStatus(200)
})

// FileBrowser rename/move (PATCH with { destination })
app.patch('/app/filebrowser/api/resources/*', (req, res) => {
  const store = currentStore()
  const { tree, contents } = store.files
  const full = fbNormalize(req.params[0] || '')
  let destination = req.body?.destination || ''
  if (req.query.action === 'rename') {
    try { destination = decodeURIComponent(String(req.query.destination || '')) }
    catch { return res.sendStatus(400) }
  }
  const dest = fbNormalize(destination)
  if (!dest || dest === '/') return res.sendStatus(400)
  const parent = fbParent(full)
  const entry = (tree[parent] || []).find(e => e.path === full)
  if (!entry) return res.sendStatus(404)
  const newParent = fbParent(dest)
  if (!tree[newParent]) return res.sendStatus(404)
  if (dest === full) return res.sendStatus(200)
  const replaced = tree[newParent].find(e => e.path === dest)
  if (replaced && (replaced.isDir || req.query.override !== 'true')) return res.sendStatus(409)
  if (store.files.uploaded.has(dest)) {
    store.files.bytes -= Buffer.byteLength(contents[dest])
    store.files.uploaded.delete(dest)
  }
  tree[parent] = tree[parent].filter(e => e !== entry)
  tree[newParent] = tree[newParent].filter(e => e.path !== dest)
  tree[newParent].push(entry)
  if (store.files.uploaded.delete(full)) store.files.uploaded.add(dest)
  const newName = fbBase(dest)
  entry.name = newName
  entry.path = dest
  entry.modified = new Date().toISOString()
  entry.type = entry.isDir ? '' : fbType(newName)
  if (contents[full] !== undefined) { contents[dest] = contents[full]; delete contents[full] }
  res.sendStatus(200)
})

// FileBrowser raw file content (text reads, blob/stream fetches)
app.get('/app/filebrowser/api/raw/*', (req, res) => {
  const full = fbNormalize(req.params[0] || '')
  // Curated binary files live on disk and stream directly, with HTTP Range
  // support so audio/video can seek and play in the browser.
  if (diskFilePaths[full]) {
    const abs = diskFilePaths[full]
    let stat
    try { stat = fsSync.statSync(abs) } catch { return res.status(404).send('File not found') }
    res.type(fbContentType(fbBase(full)))
    res.setHeader('Accept-Ranges', 'bytes')
    const range = req.headers.range
    if (range) {
      const m = /bytes=(\d*)-(\d*)/.exec(range)
      let start = m && m[1] ? parseInt(m[1], 10) : 0
      let end = m && m[2] ? parseInt(m[2], 10) : stat.size - 1
      if (isNaN(start) || start < 0) start = 0
      if (isNaN(end) || end >= stat.size) end = stat.size - 1
      if (start > end) { res.status(416).setHeader('Content-Range', `bytes */${stat.size}`); return res.end() }
      res.status(206)
      res.setHeader('Content-Range', `bytes ${start}-${end}/${stat.size}`)
      res.setHeader('Content-Length', end - start + 1)
      return fsSync.createReadStream(abs, { start, end })
        .on('error', () => { if (!res.headersSent) res.status(404).end() }).pipe(res)
    }
    res.setHeader('Content-Length', stat.size)
    return fsSync.createReadStream(abs)
      .on('error', () => { if (!res.headersSent) res.status(404).send('File not found') }).pipe(res)
  }
  const content = currentStore().files.contents[full]
  if (content === undefined) {
    // Seeded media entries have no backing file (only curated demo/content or
    // demo/files do). Serve generated-but-real media so nothing 404s and the
    // players never hit "no supported source".
    const type = fbType(fbBase(full))
    if (type === 'audio') {
      const wav = demoToneWav()
      res.type('audio/wav')
      res.setHeader('Content-Length', wav.length)
      return res.send(wav)
    }
    if (type === 'image') {
      res.type('image/svg+xml')
      return res.send(demoPlaceholderSvg(fbBase(full)))
    }
    return res.status(404).send('File not found')
  }
  res.type(fbContentType(fbBase(full)))
  res.send(Buffer.isBuffer(content) ? content : String(content))
})

// A short, real WAV (two-note ambient pad) used for seeded audio entries that
// have no curated file behind them — so pressing play always plays something.
let __demoToneWav = null
function demoToneWav() {
  if (__demoToneWav) return __demoToneWav
  const rate = 22050
  const secs = 20
  const n = rate * secs
  const buf = Buffer.alloc(44 + n * 2)
  buf.write('RIFF', 0)
  buf.writeUInt32LE(36 + n * 2, 4)
  buf.write('WAVEfmt ', 8)
  buf.writeUInt32LE(16, 16)          // fmt chunk size
  buf.writeUInt16LE(1, 20)           // PCM
  buf.writeUInt16LE(1, 22)           // mono
  buf.writeUInt32LE(rate, 24)
  buf.writeUInt32LE(rate * 2, 28)    // byte rate
  buf.writeUInt16LE(2, 32)           // block align
  buf.writeUInt16LE(16, 34)          // bits per sample
  buf.write('data', 36)
  buf.writeUInt32LE(n * 2, 40)
  for (let i = 0; i < n; i++) {
    const t = i / rate
    const swell = 0.6 + 0.4 * Math.sin(2 * Math.PI * t / 8)
    const v = (Math.sin(2 * Math.PI * 220 * t) * 0.28 + Math.sin(2 * Math.PI * 277.18 * t) * 0.2) * swell
    const fade = Math.min(1, t / 0.1, (secs - t) / 0.5)
    buf.writeInt16LE(Math.round(v * fade * 16000), 44 + i * 2)
  }
  __demoToneWav = buf
  return buf
}

// Neutral placeholder for seeded image entries without a curated file.
function demoPlaceholderSvg(name) {
  const label = String(name).replace(/[<>&"]/g, '')
  return `<svg xmlns="http://www.w3.org/2000/svg" width="800" height="500" viewBox="0 0 800 500">
  <rect width="800" height="500" fill="#14161c"/>
  <rect x="24" y="24" width="752" height="452" rx="16" fill="none" stroke="#2a2e3a" stroke-width="2"/>
  <circle cx="400" cy="215" r="58" fill="none" stroke="#f7931a" stroke-width="4" opacity="0.7"/>
  <path d="M370 245 L400 185 L430 245 Z" fill="#f7931a" opacity="0.5"/>
  <text x="400" y="330" text-anchor="middle" font-family="system-ui,sans-serif" font-size="20" fill="#8a8f9a">${label}</text>
  <text x="400" y="360" text-anchor="middle" font-family="system-ui,sans-serif" font-size="13" fill="#5b6070">Archipelago demo sample</text>
</svg>`
}

// A compact description of the current (mock) node, fed to the AI assistant as
// system context in the demo so it can answer questions about this node.
function demoNodeContext() {
  const s = currentStore()
  const md = s.mockData
  const w = s.walletState
  const apps = Object.entries(md['package-data'] || {})
    .map(([id, a]) => `${a.title || id} (${a.state || 'running'})`)
  return [
    'You are the AI assistant built into this Archipelago node. This is a public DEMO node running on Bitcoin signet (testnet) with simulated data — answer as if everything is real, be concise and helpful, and feel free to discuss Bitcoin, Lightning, self-hosting and the node itself.',
    '',
    'CURRENT NODE STATE:',
    `- Software: Archipelago ${md['server-info']?.version || 'demo'}, Tor address ${md['server-info']?.['tor-address'] || 'n/a'}`,
    `- Bitcoin: signet, block height 902,418, fully synced (Bitcoin Knots).`,
    `- Wallet: on-chain ${w.onchain_sats.toLocaleString()} sats, Lightning ${w.channel_sats.toLocaleString()} sats, ecash ${w.ecash_sats.toLocaleString()} sats.`,
    `- Installed apps (${apps.length}): ${apps.join(', ')}.`,
    `- Network: FIPS encrypted mesh active with 5 authenticated peers; 12 trusted/federated nodes; Tor hidden services online.`,
    '- The user can install apps, manage their Lightning/ecash wallet, browse & buy peer files, and chat with the mesh — all from this dashboard.',
  ].join('\n')
}

// Claude API Proxy (reads ANTHROPIC_API_KEY from environment)
// Uses fetch (Node 22+) for reliable DNS resolution and streaming in Docker/Alpine
// =============================================================================

app.post('/aiui/api/claude/*', async (req, res) => {
  // DEMO: don't call the real model — return a fixed message (also avoids
  // spending the shared API key). Replies in the Anthropic streaming or
  // non-streaming shape depending on what the client asked for.
  if (DEMO) {
    const MSG = 'Not available in demo, check out the previous chats to experience AIUI'
    if (req.body && req.body.stream === false) {
      return res.json({
        id: 'msg_demo', type: 'message', role: 'assistant', model: 'claude-demo',
        content: [{ type: 'text', text: MSG }],
        stop_reason: 'end_turn', stop_sequence: null,
        usage: { input_tokens: 1, output_tokens: 14 },
      })
    }
    res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' })
    const send = (event, data) => res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`)
    send('message_start', { type: 'message_start', message: { id: 'msg_demo', type: 'message', role: 'assistant', model: 'claude-demo', content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 1, output_tokens: 0 } } })
    send('content_block_start', { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } })
    send('content_block_delta', { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: MSG } })
    send('content_block_stop', { type: 'content_block_stop', index: 0 })
    send('message_delta', { type: 'message_delta', delta: { stop_reason: 'end_turn', stop_sequence: null }, usage: { output_tokens: 14 } })
    send('message_stop', { type: 'message_stop' })
    return res.end()
  }
  const apiKey = process.env.ANTHROPIC_API_KEY
  if (!apiKey) {
    return res.status(500).json({
      type: 'error',
      error: { type: 'configuration_error', message: 'ANTHROPIC_API_KEY not configured on server' }
    })
  }

  const apiPath = '/' + req.params[0]

  // Clean request body for Anthropic API
  const body = req.body
  if (body) {
    if (!body.max_tokens) body.max_tokens = 4096
    // Fix model IDs — AIUI may send short names
    if (body.model && !body.model.includes('-2')) {
      const modelMap = {
        'claude-haiku-4.5': 'claude-haiku-4-5-20251001',
        'claude-haiku-4-5': 'claude-haiku-4-5-20251001',
        'claude-sonnet-4-5': 'claude-sonnet-4-5-20250514',
        'claude-sonnet-4.5': 'claude-sonnet-4-5-20250514',
      }
      if (modelMap[body.model]) body.model = modelMap[body.model]
    }
    // Strip AIUI-specific fields that Anthropic API rejects
    delete body.webSearch
    delete body.webResults
    delete body.context

    // DEMO: ground the assistant in THIS node's (mock) state so it answers
    // questions about the local node, its apps, wallet and Bitcoin like a real
    // Archipelago assistant — no /seed needed.
    if (DEMO) {
      const ctx = demoNodeContext()
      if (typeof body.system === 'string') body.system = ctx + '\n\n' + body.system
      else if (Array.isArray(body.system)) body.system = [{ type: 'text', text: ctx }, ...body.system]
      else body.system = ctx
    }
  }

  const bodyStr = JSON.stringify(body)
  const url = `https://api.anthropic.com${apiPath}`
  console.log(`[Claude Proxy] → POST ${url} (${bodyStr.length} bytes, model: ${body?.model || 'unknown'})`)

  try {
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), 60000)

    const apiRes = await fetch(url, {
      method: 'POST',
      signal: controller.signal,
      headers: {
        'Content-Type': 'application/json',
        'x-api-key': apiKey,
        'anthropic-version': '2023-06-01',
      },
      body: bodyStr,
    })

    clearTimeout(timeout)
    console.log(`[Claude Proxy] ← ${apiRes.status}`)

    // Forward status and headers
    res.status(apiRes.status)
    for (const [key, value] of apiRes.headers.entries()) {
      // Skip hop-by-hop headers
      if (!['transfer-encoding', 'connection', 'keep-alive'].includes(key.toLowerCase())) {
        res.setHeader(key, value)
      }
    }

    // Stream the response body
    if (apiRes.body) {
      const reader = apiRes.body.getReader()
      const pump = async () => {
        while (true) {
          const { done, value } = await reader.read()
          if (done) { res.end(); return }
          if (!res.writableEnded) res.write(value)
        }
      }
      pump().catch((err) => {
        console.error('[Claude Proxy] Stream error:', err.message)
        if (!res.writableEnded) res.end()
      })
    } else {
      res.end()
    }
  } catch (err) {
    const msg = err.name === 'AbortError' ? 'Request timed out (60s)' : (err.message || 'Unknown error')
    console.error(`[Claude Proxy] Error: ${msg}`)
    if (!res.headersSent) {
      res.status(502).json({
        type: 'error',
        error: { type: 'proxy_error', message: msg }
      })
    }
  }
})

// Ollama (local AI) proxy — forwards to localhost:11434
app.all('/aiui/api/ollama/*', (req, res) => {
  const ollamaPath = '/' + req.params[0]
  const bodyStr = JSON.stringify(req.body)

  const options = {
    hostname: '127.0.0.1',
    port: 11434,
    path: ollamaPath,
    method: req.method,
    headers: {
      'Content-Type': 'application/json',
      'Content-Length': Buffer.byteLength(bodyStr),
    },
  }

  const proxyReq = http.request(options, (proxyRes) => {
    res.writeHead(proxyRes.statusCode, proxyRes.headers)
    proxyRes.pipe(res)
  })

  proxyReq.on('error', (err) => {
    const msg = err.message || err.code || 'Ollama not available'
    console.error('[Ollama Proxy] Error:', msg)
    if (!res.headersSent) {
      res.status(502).json({ error: msg })
    }
  })

  if (req.method !== 'GET' && req.method !== 'HEAD') {
    proxyReq.write(bodyStr)
  }
  proxyReq.end()
})

// =============================================================================
// Ollama Local AI Proxy (forwards to Ollama on localhost:11434)
// =============================================================================
app.all('/api/ollama/*', (req, res) => {
  const ollamaPath = '/' + req.params[0]
  const isPost = req.method === 'POST'
  const bodyStr = isPost ? JSON.stringify(req.body) : null

  const options = {
    hostname: '127.0.0.1',
    port: 11434,
    path: ollamaPath,
    method: req.method,
    headers: { 'Content-Type': 'application/json' },
  }

  const proxyReq = http.request(options, (proxyRes) => {
    res.writeHead(proxyRes.statusCode, proxyRes.headers)
    proxyRes.pipe(res)
  })

  proxyReq.on('error', (err) => {
    const msg = err.message || err.code || 'Ollama not available'
    console.error('[Ollama Proxy] Error:', msg)
    if (!res.headersSent) {
      res.status(502).json({ error: msg })
    }
  })

  if (bodyStr) proxyReq.write(bodyStr)
  proxyReq.end()
})

// Web search stub (no search engine configured in demo)
app.get('/api/web-search', (req, res) => {
  res.json({ results: [] })
})

// TMDB API stub (no TMDB key in demo)
app.get('/api/tmdb/*', (req, res) => {
  res.json({ results: [] })
})

// Catch-all for unimplemented API endpoints (return JSON, not HTML)
app.all('/api/*', (req, res) => {
  res.status(404).json({ error: 'Not available in demo mode' })
})
app.all('/aiui/api/*', (req, res) => {
  res.status(404).json({ error: 'Not available in demo mode' })
})

// =============================================================================
// Mock ThunderHub UI + Lightning API (no Docker required)
// =============================================================================

const MOCK_LND_DATA = {
  info: {
    alias: 'archy-signet',
    color: '#f7931a',
    public_key: '03a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456',
    num_active_channels: 4,
    num_inactive_channels: 1,
    num_pending_channels: 1,
    block_height: 892451,
    synced_to_chain: true,
    synced_to_graph: true,
    version: '0.17.4-beta commit=v0.17.4-beta',
    chains: [{ chain: 'bitcoin', network: 'signet' }],
    uris: ['03a1b2c3@archy-signet.onion:9735'],
  },
  balance: {
    total_balance: 2_450_000,
    confirmed_balance: 2_350_000,
    unconfirmed_balance: 100_000,
  },
  channelBalance: {
    local_balance: { sat: 8_250_000 },
    remote_balance: { sat: 11_750_000 },
    pending_open_local_balance: { sat: 500_000 },
    pending_open_remote_balance: { sat: 0 },
  },
  channels: [
    { chan_id: '840921088114688', remote_pubkey: '02778f4a4e...acinq', capacity: 5_000_000, local_balance: 2_450_000, remote_balance: 2_550_000, active: true, peer_alias: 'ACINQ Signet', total_satoshis_sent: 850_000, total_satoshis_received: 1_200_000, uptime: 604800, lifetime: 2592000 },
    { chan_id: '840921088114689', remote_pubkey: '03abcdef12...wos', capacity: 2_000_000, local_balance: 1_200_000, remote_balance: 800_000, active: true, peer_alias: 'WalletOfSatoshi', total_satoshis_sent: 350_000, total_satoshis_received: 500_000, uptime: 259200, lifetime: 1296000 },
    { chan_id: '840921088114690', remote_pubkey: '02fedcba98...voltage', capacity: 10_000_000, local_balance: 4_500_000, remote_balance: 5_500_000, active: true, peer_alias: 'Voltage', total_satoshis_sent: 2_100_000, total_satoshis_received: 1_800_000, uptime: 518400, lifetime: 2592000 },
    { chan_id: '840921088114691', remote_pubkey: '03456789ab...kraken', capacity: 3_000_000, local_balance: 100_000, remote_balance: 2_900_000, active: true, peer_alias: 'Kraken', total_satoshis_sent: 50_000, total_satoshis_received: 120_000, uptime: 86400, lifetime: 604800 },
    { chan_id: '840921088114692', remote_pubkey: '02112233aa...old', capacity: 1_000_000, local_balance: 0, remote_balance: 1_000_000, active: false, peer_alias: 'OldPeer-Offline', total_satoshis_sent: 0, total_satoshis_received: 0, uptime: 0, lifetime: 5184000 },
  ],
  pendingChannels: {
    pending_open_channels: [
      { channel: { remote_node_pub: '03ffeeddcc...newpeer', capacity: 500_000, local_balance: 500_000, remote_balance: 0 }, confirmation_height: 892452 },
    ],
    pending_closing_channels: [],
    pending_force_closing_channels: [],
    waiting_close_channels: [],
  },
  invoices: [
    { memo: 'Channel opening fee', value: 50_000, settled: true, creation_date: Math.floor(Date.now()/1000) - 86400, settle_date: Math.floor(Date.now()/1000) - 85800, payment_request: 'lnsb500000n1pjtest...truncated', state: 'SETTLED', amt_paid_sat: 50_000, r_hash: Buffer.from('aabbccdd01').toString('hex') },
    { memo: 'Test payment', value: 1_000, settled: true, creation_date: Math.floor(Date.now()/1000) - 7200, settle_date: Math.floor(Date.now()/1000) - 7100, payment_request: 'lnsb10000n1pjtest2...truncated', state: 'SETTLED', amt_paid_sat: 1_000, r_hash: Buffer.from('aabbccdd02').toString('hex') },
    { memo: 'Coffee payment', value: 5_000, settled: true, creation_date: Math.floor(Date.now()/1000) - 3600, settle_date: Math.floor(Date.now()/1000) - 3500, payment_request: 'lnsb50000n1pjtest3...truncated', state: 'SETTLED', amt_paid_sat: 5_000, r_hash: Buffer.from('aabbccdd03').toString('hex') },
    { memo: 'Donation', value: 21_000, settled: false, creation_date: Math.floor(Date.now()/1000) - 600, settle_date: 0, payment_request: 'lnsb210000n1pjtest4...truncated', state: 'OPEN', amt_paid_sat: 0, r_hash: Buffer.from('aabbccdd04').toString('hex') },
  ],
  payments: [
    { payment_hash: 'ff00112233', value_sat: 10_000, fee_sat: 3, status: 'SUCCEEDED', creation_date: Math.floor(Date.now()/1000) - 43200, payment_request: 'lnsb100000n1pjpay1...', failure_reason: 'FAILURE_REASON_NONE' },
    { payment_hash: 'ff00112234', value_sat: 100_000, fee_sat: 12, status: 'SUCCEEDED', creation_date: Math.floor(Date.now()/1000) - 21600, payment_request: 'lnsb1000000n1pjpay2...', failure_reason: 'FAILURE_REASON_NONE' },
    { payment_hash: 'ff00112235', value_sat: 500, fee_sat: 1, status: 'SUCCEEDED', creation_date: Math.floor(Date.now()/1000) - 1800, payment_request: 'lnsb5000n1pjpay3...', failure_reason: 'FAILURE_REASON_NONE' },
  ],
  forwarding: [
    { chan_id_in: '840921088114688', chan_id_out: '840921088114690', amt_in: 10_012, amt_out: 10_000, fee: 12, timestamp_ns: (Date.now() - 7200000) * 1e6 },
    { chan_id_in: '840921088114690', chan_id_out: '840921088114689', amt_in: 5_005, amt_out: 5_000, fee: 5, timestamp_ns: (Date.now() - 3600000) * 1e6 },
    { chan_id_in: '840921088114689', chan_id_out: '840921088114691', amt_in: 25_025, amt_out: 25_000, fee: 25, timestamp_ns: (Date.now() - 1200000) * 1e6 },
  ],
}

// ThunderHub mock web UI
app.get('/app/thunderhub/', (req, res) => {
  const d = MOCK_LND_DATA
  const totalCap = d.channels.reduce((s, c) => s + c.capacity, 0)
  const totalLocal = d.channels.reduce((s, c) => s + c.local_balance, 0)
  const totalRemote = d.channels.reduce((s, c) => s + c.remote_balance, 0)
  const totalFees = d.forwarding.reduce((s, f) => s + f.fee, 0)
  const channelRows = d.channels.map(c => `
    <tr>
      <td>${c.peer_alias}</td>
      <td>${(c.capacity/1e6).toFixed(1)}M</td>
      <td><div style="display:flex;gap:4px;align-items:center"><div style="background:#4ade80;height:8px;width:${Math.round(c.local_balance/c.capacity*100)}%;border-radius:4px"></div><span style="font-size:11px;opacity:.6">${(c.local_balance/1e3).toFixed(0)}k</span></div></td>
      <td><div style="display:flex;gap:4px;align-items:center"><div style="background:#3b82f6;height:8px;width:${Math.round(c.remote_balance/c.capacity*100)}%;border-radius:4px"></div><span style="font-size:11px;opacity:.6">${(c.remote_balance/1e3).toFixed(0)}k</span></div></td>
      <td><span style="color:${c.active ? '#4ade80' : '#ef4444'}">${c.active ? 'Active' : 'Offline'}</span></td>
    </tr>`).join('')
  const invoiceRows = d.invoices.slice().reverse().map(inv => `
    <tr>
      <td>${inv.memo}</td>
      <td>${inv.value.toLocaleString()} sats</td>
      <td><span style="color:${inv.settled ? '#4ade80' : '#fb923c'}">${inv.settled ? 'Settled' : 'Open'}</span></td>
      <td>${new Date(inv.creation_date * 1000).toLocaleString()}</td>
    </tr>`).join('')
  const paymentRows = d.payments.map(p => `
    <tr>
      <td>${p.payment_hash.slice(0,12)}...</td>
      <td>${p.value_sat.toLocaleString()} sats</td>
      <td>${p.fee_sat} sats</td>
      <td style="color:#4ade80">Succeeded</td>
    </tr>`).join('')

  res.type('html').send(`<!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ThunderHub — archy-signet</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#0f0f1a;color:#e0e0e0;font-family:system-ui,-apple-system,sans-serif;padding:24px}
h1{font-size:22px;margin-bottom:4px;color:#fb923c}
.subtitle{color:rgba(255,255,255,.5);font-size:13px;margin-bottom:24px}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:16px;margin-bottom:32px}
.stat{background:rgba(255,255,255,.05);border:1px solid rgba(255,255,255,.1);border-radius:12px;padding:16px}
.stat .label{font-size:11px;text-transform:uppercase;letter-spacing:1px;color:rgba(255,255,255,.4);margin-bottom:4px}
.stat .value{font-size:24px;font-weight:600}
.stat .value.green{color:#4ade80}.stat .value.orange{color:#fb923c}.stat .value.blue{color:#3b82f6}
h2{font-size:16px;margin:24px 0 12px;color:rgba(255,255,255,.8)}
table{width:100%;border-collapse:collapse;font-size:13px}
th{text-align:left;padding:8px 12px;border-bottom:1px solid rgba(255,255,255,.1);color:rgba(255,255,255,.4);font-weight:500;font-size:11px;text-transform:uppercase;letter-spacing:.5px}
td{padding:8px 12px;border-bottom:1px solid rgba(255,255,255,.05)}
tr:hover td{background:rgba(255,255,255,.02)}
.section{background:rgba(0,0,0,.3);border:1px solid rgba(255,255,255,.08);border-radius:12px;padding:20px;margin-bottom:20px}
.badge{display:inline-block;background:rgba(247,147,26,.15);color:#fb923c;padding:2px 8px;border-radius:6px;font-size:11px;margin-left:8px}
</style></head><body>
<h1>ThunderHub<span class="badge">signet</span></h1>
<div class="subtitle">${d.info.alias} &mdash; block ${d.info.block_height.toLocaleString()} &mdash; ${d.info.num_active_channels} active channels</div>

<div class="grid">
  <div class="stat"><div class="label">On-chain Balance</div><div class="value orange">${(d.balance.confirmed_balance/1e6).toFixed(2)}M sats</div></div>
  <div class="stat"><div class="label">Channel Capacity</div><div class="value">${(totalCap/1e6).toFixed(1)}M sats</div></div>
  <div class="stat"><div class="label">Local Balance</div><div class="value green">${(totalLocal/1e6).toFixed(1)}M sats</div></div>
  <div class="stat"><div class="label">Remote Balance</div><div class="value blue">${(totalRemote/1e6).toFixed(1)}M sats</div></div>
  <div class="stat"><div class="label">Routing Fees Earned</div><div class="value green">${totalFees} sats</div></div>
  <div class="stat"><div class="label">Payments Sent</div><div class="value">${d.payments.length}</div></div>
</div>

<div class="section">
<h2>Channels (${d.channels.length})</h2>
<table><thead><tr><th>Peer</th><th>Capacity</th><th>Local</th><th>Remote</th><th>Status</th></tr></thead><tbody>${channelRows}</tbody></table>
</div>

<div class="section">
<h2>Recent Invoices</h2>
<table><thead><tr><th>Memo</th><th>Amount</th><th>Status</th><th>Created</th></tr></thead><tbody>${invoiceRows}</tbody></table>
</div>

<div class="section">
<h2>Recent Payments</h2>
<table><thead><tr><th>Hash</th><th>Amount</th><th>Fee</th><th>Status</th></tr></thead><tbody>${paymentRows}</tbody></table>
</div>

<div class="section">
<h2>Forwarding History</h2>
<table><thead><tr><th>In Channel</th><th>Out Channel</th><th>Amount</th><th>Fee</th><th>Time</th></tr></thead><tbody>
${d.forwarding.map(f => {
  const inPeer = d.channels.find(c => c.chan_id === f.chan_id_in)?.peer_alias || f.chan_id_in
  const outPeer = d.channels.find(c => c.chan_id === f.chan_id_out)?.peer_alias || f.chan_id_out
  return `<tr><td>${inPeer}</td><td>${outPeer}</td><td>${f.amt_in.toLocaleString()} sats</td><td>${f.fee} sats</td><td>${new Date(f.timestamp_ns/1e6).toLocaleString()}</td></tr>`
}).join('')}
</tbody></table>
</div>

<p style="text-align:center;color:rgba(255,255,255,.25);font-size:12px;margin-top:32px">Mock ThunderHub &mdash; Archipelago Dev Mode &mdash; No Docker Required</p>
</body></html>`)
})

// ThunderHub API stubs (for any programmatic access)
app.get('/app/thunderhub/api/info', (req, res) => res.json(MOCK_LND_DATA.info))
app.get('/app/thunderhub/api/balance', (req, res) => res.json(MOCK_LND_DATA.balance))
app.get('/app/thunderhub/api/channels', (req, res) => res.json(MOCK_LND_DATA.channels))
app.get('/app/thunderhub/api/invoices', (req, res) => res.json(MOCK_LND_DATA.invoices))
app.get('/app/thunderhub/api/payments', (req, res) => res.json(MOCK_LND_DATA.payments))
app.get('/app/thunderhub/api/forwards', (req, res) => res.json(MOCK_LND_DATA.forwarding))

// ── Demo placeholder UIs for the extra installed apps ────────────────────────
// Static dashboards rendered with demoAppShell so every installed app opens to
// something plausible in the in-app iframe. Registered before the generic
// /app/:id notice handler so these win.
const DEMO_APP_PAGES = {
  // Placeholder LND dashboard — the real lnd-ui shell reads poorly inside the
  // demo iframe. Numbers stay consistent with the /proxy/lnd/v1/* mocks.
  lnd: () => demoAppShell('Lightning Network Daemon', 'archipelago-lnd · v0.18.3-beta · signet', '/assets/img/app-icons/lnd.png', `
    <div class="grid">
      <div class="card"><div class="k">Status</div><div class="v"><span class="badge">Running · synced</span></div></div>
      <div class="card"><div class="k">On-chain</div><div class="v">2,450,000 sats</div></div>
      <div class="card"><div class="k">Lightning (local)</div><div class="v">8,250,000 sats</div></div>
      <div class="card"><div class="k">Inbound capacity</div><div class="v">11,750,000 sats</div></div>
    </div>
    <div class="grid">
      <div class="card"><div class="k">Channels</div><div class="v">4 active</div></div>
      <div class="card"><div class="k">Peers</div><div class="v">11</div></div>
      <div class="card"><div class="k">Block height</div><div class="v">902,418</div></div>
    </div>
    <div class="card">
      <div class="k" style="margin-bottom:6px">Channels</div>
      <table>
        <tr><th>Peer</th><th>Capacity</th><th>Local / Remote</th><th style="width:30%">Balance</th></tr>
        <tr><td>ACINQ</td><td>5,000,000</td><td>2,450,000 / 2,550,000</td><td><div class="bar"><i style="width:49%"></i></div></td></tr>
        <tr><td>Voltage</td><td>10,000,000</td><td>4,500,000 / 5,500,000</td><td><div class="bar"><i style="width:45%"></i></div></td></tr>
        <tr><td>Kraken</td><td>3,000,000</td><td>1,800,000 / 1,200,000</td><td><div class="bar"><i style="width:60%"></i></div></td></tr>
        <tr><td>Wallet of Satoshi</td><td>2,000,000</td><td>1,200,000 / 800,000</td><td><div class="bar"><i style="width:60%"></i></div></td></tr>
      </table>
    </div>
    <div class="card" style="margin-top:14px">
      <div class="k" style="margin-bottom:6px">Node URI</div>
      <div class="v mono">02c9f0a1…e47b@lnd7f3a2c9d1b4e8f6.onion:9735</div>
    </div>`),
  'btcpay-server': () => demoAppShell('BTCPay Server', 'Self-hosted payment processor · signet', '/assets/img/app-icons/btcpay-server.png', `
    <div class="grid">
      <div class="card"><div class="k">Store</div><div class="v">Archipelago Shop</div></div>
      <div class="card"><div class="k">Invoices (30d)</div><div class="v">47</div></div>
      <div class="card"><div class="k">Settled</div><div class="v">1,284,500 sats</div></div>
      <div class="card"><div class="k">Lightning</div><div class="v"><span class="badge">Connected</span></div></div>
    </div>
    <div class="card"><table>
      <tr><th>Invoice</th><th>Amount</th><th>Status</th><th>Created</th></tr>
      <tr><td>INV-0047 · Sticker pack</td><td>21,000 sats</td><td>Settled</td><td>2h ago</td></tr>
      <tr><td>INV-0046 · Node consult</td><td>250,000 sats</td><td>Settled</td><td>9h ago</td></tr>
      <tr><td>INV-0045 · Coffee</td><td>5,500 sats</td><td>Settled</td><td>1d ago</td></tr>
      <tr><td>INV-0044 · Merch bundle</td><td>84,000 sats</td><td>Expired</td><td>2d ago</td></tr>
    </table></div>`),
  grafana: () => demoAppShell('Grafana', 'Dashboards · node metrics', '/assets/img/app-icons/grafana.png', `
    <div class="grid">
      <div class="card"><div class="k">Dashboards</div><div class="v">6</div></div>
      <div class="card"><div class="k">Data sources</div><div class="v">3</div></div>
      <div class="card"><div class="k">Alerts firing</div><div class="v">0</div></div>
      <div class="card"><div class="k">Uptime</div><div class="v">99.98%</div></div>
    </div>
    <div class="card"><table>
      <tr><th>Dashboard</th><th>Panels</th><th>Last viewed</th></tr>
      <tr><td>Bitcoin Node Overview</td><td>14</td><td>12m ago</td></tr>
      <tr><td>Lightning Routing</td><td>9</td><td>1h ago</td></tr>
      <tr><td>System Resources</td><td>11</td><td>3h ago</td></tr>
      <tr><td>Tor &amp; Network</td><td>7</td><td>1d ago</td></tr>
    </table></div>`),
  nextcloud: () => demoAppShell('Nextcloud', 'Files · Calendar · Contacts', '/assets/img/app-icons/nextcloud.webp', `
    <div class="grid">
      <div class="card"><div class="k">Storage used</div><div class="v">182 GB / 1.8 TB</div><div class="bar"><i style="width:10%"></i></div></div>
      <div class="card"><div class="k">Files</div><div class="v">12,847</div></div>
      <div class="card"><div class="k">Shares</div><div class="v">23</div></div>
      <div class="card"><div class="k">Users</div><div class="v">3</div></div>
    </div>
    <div class="card"><table>
      <tr><th>Recent file</th><th>Size</th><th>Modified</th></tr>
      <tr><td>node-backup-2026-07.tar.gz</td><td>2.3 GB</td><td>Today</td></tr>
      <tr><td>Sovereign Computing.pdf</td><td>4.8 MB</td><td>Yesterday</td></tr>
      <tr><td>family-photos-june/</td><td>1.2 GB</td><td>3d ago</td></tr>
    </table></div>`),
  jellyfin: () => demoAppShell('Jellyfin', 'The Free Software Media System', '/assets/img/app-icons/jellyfin.webp', `
    <div class="grid">
      <div class="card"><div class="k">Movies</div><div class="v">214</div></div>
      <div class="card"><div class="k">Shows</div><div class="v">38</div></div>
      <div class="card"><div class="k">Music albums</div><div class="v">156</div></div>
      <div class="card"><div class="k">Active streams</div><div class="v">1</div></div>
    </div>
    <div class="card"><table>
      <tr><th>Recently added</th><th>Type</th><th>Added</th></tr>
      <tr><td>The Bitcoin Standard (audiobook)</td><td>Audio</td><td>Today</td></tr>
      <tr><td>Lightning Payment Demo</td><td>Video</td><td>2d ago</td></tr>
      <tr><td>Node Unboxing Timelapse</td><td>Video</td><td>5d ago</td></tr>
    </table></div>`),
  vaultwarden: () => demoAppShell('Vaultwarden', 'Bitwarden-compatible password manager', '/assets/img/app-icons/vaultwarden.webp', `
    <div class="grid">
      <div class="card"><div class="k">Vault items</div><div class="v">184</div></div>
      <div class="card"><div class="k">Logins</div><div class="v">142</div></div>
      <div class="card"><div class="k">Secure notes</div><div class="v">28</div></div>
      <div class="card"><div class="k">Status</div><div class="v"><span class="badge">Vault locked</span></div></div>
    </div>
    <div class="card">
      <div class="k" style="margin-bottom:10px">Security report</div>
      <table>
        <tr><td>Weak passwords</td><td>3</td></tr>
        <tr><td>Reused passwords</td><td>1</td></tr>
        <tr><td>Exposed in breaches</td><td>0</td></tr>
        <tr><td>Two-factor enabled</td><td>Yes</td></tr>
      </table>
    </div>`),
  'nostr-rs-relay': () => demoAppShell('Nostr Relay', 'wss://relay.archipelago.lan', '/assets/img/app-icons/nostrudel.svg', `
    <div class="grid">
      <div class="card"><div class="k">Events stored</div><div class="v">48,213</div></div>
      <div class="card"><div class="k">Connections</div><div class="v">17</div></div>
      <div class="card"><div class="k">Subscriptions</div><div class="v">42</div></div>
      <div class="card"><div class="k">DB size</div><div class="v">312 MB</div></div>
    </div>
    <div class="card"><table>
      <tr><th>Kind</th><th>Description</th><th>Events (24h)</th></tr>
      <tr><td>1</td><td>Short text notes</td><td>1,824</td></tr>
      <tr><td>0</td><td>Profile metadata</td><td>96</td></tr>
      <tr><td>7</td><td>Reactions</td><td>3,412</td></tr>
      <tr><td>4</td><td>Encrypted DMs</td><td>58</td></tr>
    </table></div>`),
  searxng: () => demoAppShell('SearXNG', 'Privacy-respecting metasearch', '/assets/img/app-icons/searxng.png', `
    <div class="card" style="margin-bottom:18px;text-align:center;padding:28px">
      <input placeholder="Search privately…" style="width:100%;max-width:480px;background:rgba(255,255,255,.06);border:1px solid rgba(255,255,255,.14);border-radius:10px;padding:12px 16px;color:#f2f2f4;font-size:15px;outline:none">
      <div class="k" style="margin-top:12px">No queries logged · no profiling · results proxied</div>
    </div>
    <div class="grid">
      <div class="card"><div class="k">Engines enabled</div><div class="v">28</div></div>
      <div class="card"><div class="k">Queries today</div><div class="v">64</div></div>
      <div class="card"><div class="k">Avg response</div><div class="v">0.8s</div></div>
      <div class="card"><div class="k">Tracking</div><div class="v"><span class="badge">Zero</span></div></div>
    </div>`),
  'uptime-kuma': () => demoAppShell('Uptime Kuma', 'Service monitoring', '/assets/img/app-icons/uptime-kuma.webp', `
    <div class="grid">
      <div class="card"><div class="k">Monitors</div><div class="v">8</div></div>
      <div class="card"><div class="k">Up</div><div class="v">8</div></div>
      <div class="card"><div class="k">Avg uptime (30d)</div><div class="v">99.94%</div></div>
      <div class="card"><div class="k">Incidents</div><div class="v">1</div></div>
    </div>
    <div class="card"><table>
      <tr><th>Monitor</th><th>Status</th><th>Uptime</th><th>Ping</th></tr>
      <tr><td>Bitcoin RPC</td><td><span class="badge">Up</span></td><td>100%</td><td>4 ms</td></tr>
      <tr><td>LND REST</td><td><span class="badge">Up</span></td><td>99.9%</td><td>7 ms</td></tr>
      <tr><td>Electrum server</td><td><span class="badge">Up</span></td><td>99.8%</td><td>11 ms</td></tr>
      <tr><td>Tor hidden service</td><td><span class="badge">Up</span></td><td>99.7%</td><td>380 ms</td></tr>
      <tr><td>Mempool web</td><td><span class="badge">Up</span></td><td>100%</td><td>6 ms</td></tr>
    </table></div>`),
}
for (const [id, page] of Object.entries(DEMO_APP_PAGES)) {
  app.get([`/app/${id}`, `/app/${id}/`], (_req, res) => res.type('html').send(page()))
}

// Generic app shell for any launched app without a dedicated mock UI — a clean
// "not interactive in the demo" notice with the app's icon. Registered after all
// specific /app/... routes so those win; only bare /app/<id>[/] reaches here.
app.get(['/app/:id', '/app/:id/'], (req, res) => {
  const id = String(req.params.id || '').replace(/[^a-z0-9._-]/gi, '')
  const title = id.replace(/[-_]/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
  res.type('html').send(`<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>${title}</title>
<style>*{margin:0;padding:0;box-sizing:border-box}html,body{height:100%}
body{background:#000;color:#f2f2f4;font-family:system-ui,-apple-system,Segoe UI,Roboto,sans-serif;display:flex;align-items:center;justify-content:center;
     background-image:radial-gradient(900px 400px at 50% -10%, rgba(247,147,26,.07), transparent 70%)}
.card{text-align:center;padding:48px 40px;max-width:380px}
img{width:84px;height:84px;border-radius:20px;object-fit:cover;margin-bottom:20px;background:rgba(255,255,255,.04);border:1px solid rgba(255,255,255,.08)}
h1{font-size:22px;font-weight:650;margin-bottom:8px}
p{color:#8a8f9a;font-size:14px;line-height:1.5}
.tag{margin-top:18px;display:inline-block;font-size:12px;color:#f7931a;border:1px solid rgba(247,147,26,.3);background:rgba(247,147,26,.1);padding:5px 12px;border-radius:999px}
</style></head><body><div class="card">
<img src="/assets/img/app-icons/${id}.png" onerror="this.onerror=null;this.src='/assets/icon/favico-black.svg'" alt="">
<h1>${title}</h1>
<p>Not available in the demo.<br>This app runs fully on a real Archipelago node.</p>
<div class="tag">Demo</div>
</div></body></html>`)
})

// Health check
app.get('/health', (req, res) => {
  res.status(200).send('healthy')
})

// ───────────────────────────────────────────────────────────────────────────
// Per-session state isolation (DEMO multi-visitor sandbox)
//
// Every mutable global (mockData, walletState, userState, mockState,
// bitcoinRelayMockState) and the filebrowser file store is partitioned per
// visitor. Handlers keep referring to those names unchanged — the names are
// Proxies that forward to the current request's store, resolved via
// AsyncLocalStorage. Outside DEMO (or outside a request, e.g. at startup) they
// resolve to a single shared `defaultStore`, so classic single-user behaviour
// is byte-for-byte preserved.
// ───────────────────────────────────────────────────────────────────────────
const stateContext = new AsyncLocalStorage()

// Build a fresh, fully-isolated state bundle from the pristine seeds.
function makeSessionStore() {
  const md = structuredClone(SEED_MOCKDATA)
  // No real runtime in DEMO → package list is the curated static app set.
  md['package-data'] = structuredClone(staticDevApps)
  return {
    mockData: md,
    walletState: structuredClone(SEED_WALLET),
    userState: seedUserState(),
    mockState: seedMockState(),
    bitcoinRelayMockState: structuredClone(SEED_BTCRELAY),
    files: { tree: structuredClone(SEED_FILES), contents: structuredClone(SEED_FILE_CONTENTS), bytes: 0, reserved: 0, uploads: new Map(), uploaded: new Set() },
    // Mesh chat mutable state: messages sent this session + attachment bytes
    // (cid → {mime, filename, b64, thumb_b64}). Per-session so one demo
    // visitor's uploads are never visible to another.
    // contacts: pubkey_hex → { alias, notes, pinned, blocked }, mirroring the
    // daemon's state.contacts map. scheduled: queued messages awaiting their
    // fire_at, mirroring svc.scheduler's list.
    mesh: { dynamic: [], blobs: {}, contacts: {}, scheduled: [], nextScheduledId: 1, overrides: {} },
    sockets: new Set(),
    lastSeen: Date.now(),
  }
}

// The shared store used in single-user mode and for any work outside a request.
const defaultStore = makeSessionStore()

function currentStore() {
  return stateContext.getStore() || defaultStore
}

// A Proxy whose every operation is delegated to currentStore()[bucket], so the
// existing handler code (`mockData['package-data']`, `walletState.x += n`, …)
// transparently reads/writes the right visitor's state.
function sessionBucketProxy(bucket) {
  const target = () => currentStore()[bucket]
  return new Proxy(Object.create(null), {
    get: (_t, k) => target()[k],
    set: (_t, k, v) => { target()[k] = v; return true },
    has: (_t, k) => k in target(),
    deleteProperty: (_t, k) => { delete target()[k]; return true },
    ownKeys: () => Reflect.ownKeys(target()),
    getOwnPropertyDescriptor: (_t, k) => {
      const d = Object.getOwnPropertyDescriptor(target(), k)
      if (d) d.configurable = true
      return d
    },
    defineProperty: (_t, k, d) => { Object.defineProperty(target(), k, d); return true },
    getPrototypeOf: () => Object.prototype,
  })
}

const mockData = sessionBucketProxy('mockData')
const walletState = sessionBucketProxy('walletState')
const userState = sessionBucketProxy('userState')
const mockState = sessionBucketProxy('mockState')

// ── Lightning macaroon rotation (mock) ──────────────────────────────────────
// Mirrors the daemon's `RotationProgress`: same step keys and the same five
// states, so the Settings section can be driven end-to-end without a node.
// Advances one step per poll rather than on a timer, which keeps it
// deterministic and makes each intermediate state actually observable.
const MACAROON_ROTATION_STEPS = [
  ['preflight', 'Check LND is healthy and record what must survive', '5 channel(s) open, 1 pending — these must be identical afterwards'],
  ['backup', 'Back up the current macaroon material', '8 file(s) copied to /var/lib/archipelago/lnd/macaroon-rotation-20260808T120000Z'],
  ['stop', 'Stop Lightning', null],
  ['remove', 'Remove the old root key and issued macaroons', '8 file(s) removed'],
  ['start', 'Start Lightning and unlock the wallet', 'Lightning is up with freshly minted credentials'],
  ['verify', 'Confirm the node and its channels are unchanged', 'same node, same 5 channel(s)'],
  ['btcpay', 'Reconnect BTCPay Server to the new credentials', 'Connection string updated. BTCPay restarts itself within a minute or two to pick it up.'],
]

const macaroonRotation = { running: false, done: 0, ok: null, startedAt: null, finishedAt: null }

function startMacaroonRotation() {
  macaroonRotation.running = true
  macaroonRotation.done = 0
  macaroonRotation.ok = null
  macaroonRotation.startedAt = new Date().toISOString()
  macaroonRotation.finishedAt = null
}

function macaroonRotationSnapshot() {
  if (macaroonRotation.running) {
    macaroonRotation.done += 1
    if (macaroonRotation.done >= MACAROON_ROTATION_STEPS.length) {
      macaroonRotation.done = MACAROON_ROTATION_STEPS.length
      macaroonRotation.running = false
      macaroonRotation.ok = true
      macaroonRotation.finishedAt = new Date().toISOString()
    }
  }
  const started = macaroonRotation.startedAt !== null
  return {
    running: macaroonRotation.running,
    ok: macaroonRotation.ok,
    started_at: macaroonRotation.startedAt,
    finished_at: macaroonRotation.finishedAt,
    error: null,
    steps: MACAROON_ROTATION_STEPS.map(([key, label, detail], i) => {
      let state = 'pending'
      if (started) {
        if (i < macaroonRotation.done) state = 'done'
        else if (i === macaroonRotation.done && macaroonRotation.running) state = 'running'
      }
      return { key, label, state, detail: state === 'done' ? detail : null }
    }),
    backup_path: started ? '/var/lib/archipelago/lnd/macaroon-rotation-20260808T120000Z' : null,
    identity_pubkey: started ? '03a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456' : null,
    channels_before: started ? 5 : null,
    channels_after: macaroonRotation.ok ? 5 : null,
    new_admin_macaroon_sha256: macaroonRotation.ok
      ? '8c19b99de4a8f5c3145d8189500089829174909ca09b48f55e0464239bd8d412'
      : null,
  }
}

// Seed for the per-session Tor services demo state (tor.list-services /
// tor.create-service / tor.delete-service round-trip against this).
function defaultTorServices() {
  return [
    { name: 'archipelago', local_port: 5678, onion_address: 'archydemox7k3pnw4hv5qz2jcbr6dwefys3ockqzf4mzjlvxot2ioad.onion', enabled: true, unauthenticated: false, protocol: false },
    { name: 'bitcoin', local_port: 8333, onion_address: 'btcmockxj4k5pnw7hv8qz9jcbr2dwefys6ockqzf1mzjlvxot5ioad.onion', enabled: true, unauthenticated: false, protocol: false },
    { name: 'lnd', local_port: 9735, onion_address: 'lndmockab3c4def5ghi6jkl7mno8pqr9stu0vwx1yz2ab3c4def5ghi.onion', enabled: true, unauthenticated: false, protocol: false },
    { name: 'electrs', local_port: 50001, onion_address: 'elecmockyz9wvu8tsr7qpo6nml5kji4hgf3edc2ba1xyz9wvu8tsr7q.onion', enabled: true, unauthenticated: true, protocol: false },
    { name: 'nostr-relay', local_port: 7777, onion_address: null, enabled: false, unauthenticated: true, protocol: false },
  ]
}
const bitcoinRelayMockState = sessionBucketProxy('bitcoinRelayMockState')

// Demo session lifecycle: keyed by the `demo_sid` cookie, capped, idle-reaped.
const demoSessions = new Map() // sid -> store
const DEMO_SESSION_TTL_MS = Number(process.env.DEMO_SESSION_TTL_MS) || 45 * 60 * 1000
const DEMO_MAX_SESSIONS = Number(process.env.DEMO_MAX_SESSIONS) || 500

function resolveSessionStore(req, res) {
  let sid = req.cookies?.demo_sid
  if (!sid || !demoSessions.has(sid)) {
    // Cap concurrent sessions: evict the oldest if we're at the limit.
    if (demoSessions.size >= DEMO_MAX_SESSIONS) {
      let oldestSid = null, oldest = Infinity
      for (const [k, s] of demoSessions) if (s.lastSeen < oldest) { oldest = s.lastSeen; oldestSid = k }
      if (oldestSid) { reapSession(oldestSid) }
    }
    sid = crypto.randomUUID()
    demoSessions.set(sid, makeSessionStore())
    res.cookie('demo_sid', sid, { httpOnly: true, sameSite: 'lax', maxAge: DEMO_SESSION_TTL_MS })
  }
  const store = demoSessions.get(sid)
  store.lastSeen = Date.now()
  store.sid = sid
  return store
}

// Resolve the session store for a WebSocket upgrade request. The HTTP layer has
// already issued the `demo_sid` cookie by the time the socket connects; if it is
// somehow absent we fall back to a fresh ephemeral store (no cookie to set here).
function wsStoreForRequest(req) {
  const raw = req.headers?.cookie || ''
  const m = raw.match(/(?:^|;\s*)demo_sid=([^;]+)/)
  const sid = m && m[1]
  if (sid && demoSessions.has(sid)) {
    const store = demoSessions.get(sid)
    store.lastSeen = Date.now()
    return store
  }
  const store = makeSessionStore()
  if (sid) demoSessions.set(sid, store)
  return store
}

function reapSession(sid) {
  const store = demoSessions.get(sid)
  if (!store) return
  for (const ws of store.sockets) { try { ws.close(4000, 'session expired') } catch { /* ignore */ } }
  demoSessions.delete(sid)
}

if (DEMO) {
  setInterval(() => {
    const now = Date.now()
    for (const [sid, store] of demoSessions) {
      if (now - store.lastSeen > DEMO_SESSION_TTL_MS) reapSession(sid)
    }
  }, 60 * 1000).unref?.()
}

// WebSocket endpoint
const server = http.createServer(app)
const wss = new WebSocketServer({ server, path: '/ws/db' })

wss.on('connection', (ws, req) => {
  console.log('[WebSocket] Client connected from', req.socket.remoteAddress)
  // Attach this socket to the visitor's session store so broadcasts only reach
  // that visitor. In non-DEMO mode every socket joins the single default store.
  const wsStore = DEMO ? wsStoreForRequest(req) : defaultStore
  wsStore.sockets.add(ws)

  // Set up ping/pong to keep connection alive
  const pingInterval = setInterval(() => {
    if (ws.readyState === 1) { // OPEN
      try {
        ws.ping()
      } catch (err) {
        console.error('[WebSocket] Ping error:', err)
        clearInterval(pingInterval)
        clearInterval(heartbeatInterval)
      }
    } else {
      clearInterval(pingInterval)
      clearInterval(heartbeatInterval)
    }
  }, 25000) // Ping every 25 seconds

  // Send periodic heartbeat data so clients (and any idle-timeout proxy in
  // front of the public demo) never see a silent socket. 20s stays well under
  // common 30–60s proxy idle timeouts that were killing demo connections.
  const heartbeatInterval = setInterval(() => {
    if (ws.readyState === 1) {
      try {
        ws.send(JSON.stringify({ type: 'heartbeat', rev: Date.now() }))
      } catch { /* ignore */ }
    }
  }, 20000)

  // Send initial data immediately (this visitor's store, not the global proxy —
  // there is no request context inside the WS connection handler).
  try {
    ws.send(JSON.stringify({
      type: 'initial',
      data: wsStore.mockData,
    }))
    console.log('[WebSocket] Initial data sent')
  } catch (err) {
    console.error('[WebSocket] Error sending initial data:', err)
  }

  ws.on('pong', () => {
    // Client responded to ping, connection is alive
  })

  ws.on('message', (message) => {
    // Handle incoming messages if needed
    try {
      const data = JSON.parse(message.toString())
      console.log('[WebSocket] Received message:', data)
    } catch (err) {
      console.error('[WebSocket] Error parsing message:', err)
    }
  })

  ws.on('close', (code, reason) => {
    console.log('[WebSocket] Client disconnected', { code, reason: reason.toString() })
    clearInterval(pingInterval)
    clearInterval(heartbeatInterval)
    wsStore.sockets.delete(ws)
  })

  ws.on('error', (error) => {
    console.error('[WebSocket Error]', error)
    clearInterval(pingInterval)
    clearInterval(heartbeatInterval)
    wsStore.sockets.delete(ws)
  })
})

// Best-effort: pull a few REAL recent txids from mempool.guide (the instance the
// demo's in-app explorer proxies) so wallet transactions deep-link to pages that
// actually resolve. Falls back to the mock hashes (already set) if offline.
// Patches the pristine seed so every session inherits them.
async function hydrateRealTestnetTxids() {
  if (!DEMO) return
  try {
    const ctrl = new AbortController()
    const t = setTimeout(() => ctrl.abort(), 4000)
    const r = await fetch('https://mempool.guide/api/mempool/recent', { signal: ctrl.signal })
    clearTimeout(t)
    if (!r.ok) return
    const recent = await r.json()
    const txids = (Array.isArray(recent) ? recent : []).map(x => x.txid).filter(Boolean)
    if (!txids.length) return
    SEED_WALLET.transactions.forEach((tx, i) => { if (txids[i]) tx.tx_hash = txids[i] })
    // Also patch the already-built default store (sessions clone from SEED at creation).
    defaultStore.walletState.transactions.forEach((tx, i) => { if (txids[i]) tx.tx_hash = txids[i] })
    console.log(`[Demo] Hydrated ${Math.min(txids.length, SEED_WALLET.transactions.length)} real testnet txids`)
  } catch { /* offline — keep mock hashes */ }
}

server.listen(PORT, process.env.MOCK_BACKEND_HOST || '0.0.0.0', async () => {
  const runtime = await isContainerRuntimeAvailable()

  // Initialize package data from Docker
  await initializePackageData()
  await hydrateRealTestnetTxids()

  console.log(`
╔════════════════════════════════════════════════════════════╗
║                                                            ║
║   🚀 Archipelago Mock Backend Server                      ║
║                                                            ║
║   RPC:       http://localhost:${PORT}/rpc/v1               ║
║   WebSocket: ws://localhost:${PORT}/ws/db                  ║
║                                                            ║
║   Dev Mode:  ${DEV_MODE.padEnd(47)}║
║   Setup:     ${userState.setupComplete ? '✅ Complete' : '❌ Not done'.padEnd(47)}║
║   Onboarding: ${userState.onboardingComplete ? '✅ Complete' : '❌ Not done'.padEnd(46)}║
║                                                            ║
║   Mock Password: ${MOCK_PASSWORD.padEnd(40)}║
║                                                            ║
║   Container Runtime: ${runtime.available ? `✅ ${runtime.runtime}`.padEnd(40) : '⏭️  Simulation mode'.padEnd(40)}║
║   Claude API Key:    ${process.env.ANTHROPIC_API_KEY ? '✅ Set (' + process.env.ANTHROPIC_API_KEY.slice(0, 12) + '...)' : '❌ Not set (chat disabled)'.padEnd(40)}║
║                                                            ║
╚════════════════════════════════════════════════════════════╝
  `)
  console.log('Mock backend is running. Press Ctrl+C to stop.\n')

  // Pre-check Anthropic API connectivity
  if (process.env.ANTHROPIC_API_KEY) {
    try {
      const dns = await import('dns')
      dns.lookup('api.anthropic.com', (err, address) => {
        if (err) {
          console.error('[Claude Proxy] ⚠ DNS lookup failed for api.anthropic.com:', err.message)
          console.error('[Claude Proxy]   Chat will fail. Check container DNS settings.')
        } else {
          console.log('[Claude Proxy] ✅ api.anthropic.com resolves to', address)
        }
      })
    } catch { /* ignore */ }
  }
  
  // Periodically update package data from Docker (merge with static dev apps)
  // Only poll if container runtime is available (avoids log spam in demo/Docker deployments)
  if (runtime.available) {
    setInterval(async () => {
      const dockerApps = await getDockerContainers()
      mockData['package-data'] = mergePackageData(dockerApps)

      // Broadcast update to connected clients
      broadcastUpdate([
        {
          op: 'replace',
          path: '/package-data',
          value: mockData['package-data']
        }
      ])
    }, 5000) // Update every 5 seconds
  }
})

process.on('SIGINT', () => {
  console.log('\n\nShutting down mock backend...')
  server.close(() => {
    console.log('Server stopped.')
    process.exit(0)
  })
})
