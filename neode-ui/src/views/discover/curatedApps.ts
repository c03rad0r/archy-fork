import type { MarketplaceApp } from './types'
import { isStoreListedApp } from '../apps/serviceNames'

const R = 'source.archipelago-foundation.org/lfg2025'

// ---------- Dynamic catalog from registry ----------
export interface CatalogFeatured {
  id: string
  banner: string
  headline: string
  description: string
  tag: string
}

export interface AppCatalog {
  version: number
  registry: string
  featured: CatalogFeatured
  apps: MarketplaceApp[]
}

/** Shape of the release-signed catalog (`releases/app-catalog.json`) served
 *  by the daemon at /api/app-catalog after release-root verification. `apps`
 *  is keyed by app id and each entry embeds the app's full manifest — the
 *  ports[] there (auth: gated/open/none) are what decides whether a port is
 *  fronted by the node's app gate (and therefore serves TLS on the same
 *  port) or published by the container as plain HTTP. */
export interface SignedAppCatalog {
  schema?: number
  updated?: string
  apps: Record<string, SignedAppEntry>
}

export interface SignedAppEntry {
  version: string
  image?: string
  manifest?: {
    app?: {
      id?: string
      name?: string
      version?: string
      description?: string
      category?: string
      container?: { image?: string }
      metadata?: { icon?: string; author?: string; repo?: string }
      ports?: { host?: number | string; container?: number | string; auth?: string }[]
    }
  }
}

/** Convert the signed catalog's keyed entries into store-listing apps.
 *  Pure — unit-tested against the live catalog's shape (Cuprate).
 *  Components and node internals never enter the listing: the signed catalog
 *  carries every manifest (including backends like mempool-api and the
 *  bundled UIs) because the node's update layer needs their pins, but the
 *  App Store publishes apps, not parts of apps. */
export function signedCatalogToApps(catalog: SignedAppCatalog): MarketplaceApp[] {
  const out: MarketplaceApp[] = []
  for (const [id, entry] of Object.entries(catalog.apps || {})) {
    if (!isStoreListedApp(id)) continue
    const app = entry.manifest?.app
    out.push({
      id,
      title: app?.name || id,
      version: entry.version || app?.version || '',
      description: app?.description || '',
      icon: app?.metadata?.icon || '/assets/icon/favico-black-v2.svg',
      author: app?.metadata?.author,
      dockerImage: entry.image || app?.container?.image || '',
      repoUrl: app?.metadata?.repo,
      category: app?.category,
      source: 'signed-catalog',
    })
  }
  return out
}

/** The daemon-verified signed catalog, kept for synchronous port-auth lookups
 *  after fetchAppCatalog() has run. Test-hookable. */
let signedCatalogCache: SignedAppCatalog | null = null

/** Port auth for an app's host port, from the signed catalog's embedded
 *  manifest. `gated`/`open` = the node's app gate owns the port and serves
 *  TLS on it; `none`/`local` = container-published plain HTTP; null = app
 *  unknown to the signed catalog (legacy curated installs). */
export function portAuth(appId: string, hostPort: number | string): string | null {
  const ports = signedCatalogCache?.apps?.[appId]?.manifest?.app?.ports
  if (!Array.isArray(ports)) return null
  const hit = ports.find(p => String(p.host) === String(hostPort))
  return hit?.auth ?? null
}

/** Whether an app's host port is fronted by the node's app gate (and so
 *  serves TLS alongside HTTP on the same port). Unknown apps are NOT —
 *  assuming TLS for a container-published port breaks it outright. */
export function portIsGateFronted(appId: string, hostPort: number | string): boolean {
  const auth = portAuth(appId, hostPort)
  return auth === 'gated' || auth === 'open'
}

export function __setSignedCatalogForTests(catalog: SignedAppCatalog | null) {
  signedCatalogCache = catalog
}

let cachedCatalog: AppCatalog | null = null
let catalogFetchedAt = 0
const CATALOG_TTL = 60 * 60 * 1000 // 1 hour cache

/** Catalog URLs for the community listing. The signed catalog is served by
 *  the backend proxy (`/api/app-catalog`) — server-side fetch bypasses CORS
 *  on the upstream Gitea and verifies the release-root signature. If the
 *  backend is offline (mid-restart etc.) the static community copy baked
 *  into the frontend build still renders the store. */
const CATALOG_URLS = [
  '/catalog.json',
]

/** Fetch app catalog from remote registry, with local fallback.
 *  Caches for 1 hour. Returns null only if ALL sources fail. */
export async function fetchAppCatalog(): Promise<AppCatalog | null> {
  // Return cache if fresh
  if (cachedCatalog && Date.now() - catalogFetchedAt < CATALOG_TTL) return cachedCatalog

  // The daemon-verified signed catalog first (release-root signature checked
  // server-side): it is what makes a newly published app appear without a
  // dashboard release. The community catalog supplies the featured banner
  // and curated copy for shared ids; signed-only ids join the listing as-is.
  let signedApps: MarketplaceApp[] = []
  let signedOk = false
  try {
    const res = await fetch('/api/app-catalog', { credentials: 'include', signal: AbortSignal.timeout(20000) })
    if (res.ok) {
      const data = await res.json() as SignedAppCatalog
      if (data.apps && !Array.isArray(data.apps)) {
        signedCatalogCache = data
        signedApps = signedCatalogToApps(data)
        signedOk = signedApps.length > 0
      }
    }
  } catch { /* fall through to the community catalog */ }

  let community: AppCatalog | null = null
  for (const url of CATALOG_URLS) {
    try {
      const res = await fetch(url, { credentials: 'include', signal: AbortSignal.timeout(20000) })
      if (!res.ok) continue
      const data = await res.json() as AppCatalog
      if (!data.apps?.length) continue
      const registry = data.registry || R
      for (const app of data.apps) {
        if (app.dockerImage && !app.dockerImage.includes('/')) {
          app.dockerImage = `${registry}/${app.dockerImage}`
        }
      }
      community = data
      break
    } catch { /* try the next source */ }
  }

  if (signedOk || community) {
    // Community copy wins for shared ids (curated descriptions, webUrl-only
    // apps); signed entries fill version/image gaps and append brand-new apps.
    const byId = new Map<string, MarketplaceApp>()
    for (const app of signedApps) byId.set(app.id, app)
    for (const app of community?.apps ?? []) {
      const existing = byId.get(app.id)
      byId.set(app.id, existing ? { ...app, version: app.version || existing.version, dockerImage: app.dockerImage || existing.dockerImage } : app)
    }
    const merged: AppCatalog = {
      version: community?.version ?? 1,
      registry: community?.registry ?? R,
      featured: community?.featured ?? { id: 'bitcoin-knots', banner: '', headline: '', description: '', tag: '' },
      apps: [...byId.values()],
    }
    cachedCatalog = merged
    catalogFetchedAt = Date.now()
    try { localStorage.setItem('archy_catalog', JSON.stringify(merged)) } catch {}
    return merged
  }

  // Try localStorage cache as final fallback
  try {
    const stored = localStorage.getItem('archy_catalog')
    if (stored) {
      cachedCatalog = JSON.parse(stored) as AppCatalog
      catalogFetchedAt = Date.now() - CATALOG_TTL + 5 * 60 * 1000 // re-check in 5 min
      return cachedCatalog
    }
  } catch (e) {
    console.warn('[catalog] localStorage fallback unreadable:', e)
  }

  console.warn('[catalog] all sources failed — using hardcoded app list')
  return null
}

// ---------- Hardcoded fallback (used when catalog.json is unavailable) ----------
export function getCuratedAppList(): MarketplaceApp[] {
  return [
    { id: 'bitcoin-knots', title: 'Bitcoin Knots', version: '28.1.0', description: 'Run a full Bitcoin node. Validate and relay blocks and transactions on the Bitcoin network.', icon: '/assets/img/app-icons/bitcoin-knots.webp', author: 'Bitcoin Knots', dockerImage: `${R}/bitcoin-knots:latest`, repoUrl: 'https://github.com/bitcoinknots/bitcoin' },
    { id: 'bitcoin-core', title: 'Bitcoin Core', version: '28.4', description: 'Reference implementation of the Bitcoin protocol. Run a full node validating and relaying blocks on the Bitcoin network.', icon: '/assets/img/app-icons/bitcoin-core.svg', author: 'Bitcoin Core contributors', dockerImage: 'docker.io/bitcoin/bitcoin:28.4', repoUrl: 'https://github.com/bitcoin/bitcoin' },
    { id: 'btcpay-server', title: 'BTCPay Server', version: '2.4.2', description: 'Self-hosted Bitcoin payment processor. Accept Bitcoin payments without intermediaries or fees.', icon: '/assets/img/app-icons/btcpay-server.png', author: 'BTCPay Server Foundation', dockerImage: 'docker.io/btcpayserver/btcpayserver:2.4.2', repoUrl: 'https://github.com/btcpayserver/btcpayserver' },
    { id: 'lnd', title: 'LND', version: '0.18.4', description: 'Lightning Network Daemon. Fast and cheap Bitcoin payments through the Lightning Network.', icon: '/assets/img/app-icons/lnd.png', author: 'Lightning Labs', dockerImage: `${R}/lnd:v0.18.4-beta`, repoUrl: 'https://github.com/lightningnetwork/lnd' },
    { id: 'mempool', title: 'Mempool Explorer', version: '3.0.0', description: 'Self-hosted Bitcoin blockchain and mempool visualizer. Monitor transactions without revealing your addresses to third parties.', icon: '/assets/img/app-icons/mempool.webp', author: 'Mempool', dockerImage: `${R}/mempool-frontend:v3.0.0`, repoUrl: 'https://github.com/mempool/mempool' },
    { id: 'homeassistant', title: 'Home Assistant', version: '2024.1', description: 'Open-source home automation. Control smart home devices privately, on your own hardware.', icon: '/assets/img/app-icons/homeassistant.png', author: 'Home Assistant', dockerImage: `${R}/home-assistant:2024.1`, repoUrl: 'https://github.com/home-assistant/core' },
    { id: 'grafana', title: 'Grafana', version: '10.2.0', description: 'Analytics and monitoring platform. Dashboards for your node metrics and system health.', icon: '/assets/img/app-icons/grafana.png', author: 'Grafana Labs', dockerImage: `${R}/grafana:10.2.0`, repoUrl: 'https://github.com/grafana/grafana' },
    { id: 'searxng', title: 'SearXNG', version: '2024.1.0', description: 'Privacy-respecting metasearch engine. Search the internet without being tracked or profiled.', icon: '/assets/img/app-icons/searxng.png', author: 'SearXNG', dockerImage: `${R}/searxng:latest`, repoUrl: 'https://github.com/searxng/searxng' },
    { id: 'ollama', title: 'Ollama', version: '0.5.4', description: 'Run AI models locally. Llama, Mistral, and more — on your hardware, completely private.', icon: '/assets/img/app-icons/ollama.png', author: 'Ollama', dockerImage: `${R}/ollama:latest`, repoUrl: 'https://github.com/ollama/ollama' },
    { id: 'nextcloud', title: 'Nextcloud', version: '29', description: 'Your own private cloud. File sync, calendars, contacts — all on your hardware.', icon: '/assets/img/app-icons/nextcloud.webp', author: 'Nextcloud', dockerImage: `${R}/nextcloud:29`, repoUrl: 'https://github.com/nextcloud/server' },
    { id: 'vaultwarden', title: 'Vaultwarden', version: '1.30.0', description: 'Self-hosted password vault. Bitwarden-compatible with zero-knowledge encryption.', icon: '/assets/img/app-icons/vaultwarden.webp', author: 'Vaultwarden', dockerImage: `${R}/vaultwarden:1.30.0-alpine`, repoUrl: 'https://github.com/dani-garcia/vaultwarden' },
    { id: 'jellyfin', title: 'Jellyfin', version: '10.8.13', description: 'Free media server. Stream your movies, music, and photos to any device.', icon: '/assets/img/app-icons/jellyfin.webp', author: 'Jellyfin', dockerImage: `${R}/jellyfin:10.8.13`, repoUrl: 'https://github.com/jellyfin/jellyfin' },
    { id: 'photoprism', title: 'PhotoPrism', version: '240915', description: 'AI-powered photo management. Organize photos with facial recognition, privately.', icon: '/assets/img/app-icons/photoprism.svg', author: 'PhotoPrism', dockerImage: `${R}/photoprism:240915`, repoUrl: 'https://github.com/photoprism/photoprism' },
    { id: 'immich', title: 'Immich', version: '1.90.0', description: 'High-performance photo and video backup. Mobile-first with ML features.', icon: '/assets/img/app-icons/immich.png', author: 'Immich', dockerImage: `${R}/immich-server:release`, repoUrl: 'https://github.com/immich-app/immich' },
    { id: 'filebrowser', title: 'File Browser', version: '2.27.0', description: 'Web-based file manager. Browse, upload, and manage files on your server.', icon: '/assets/img/app-icons/file-browser.webp', author: 'File Browser', dockerImage: `${R}/filebrowser:v2.27.0`, repoUrl: 'https://github.com/filebrowser/filebrowser' },
    { id: 'nginx-proxy-manager', title: 'Nginx Proxy Manager', version: '2.12.1', description: 'Reverse proxy with SSL. Beautiful web interface for managing proxies.', icon: '/assets/img/app-icons/nginx.svg', author: 'Nginx Proxy Manager', dockerImage: `${R}/nginx-proxy-manager:latest`, repoUrl: 'https://github.com/NginxProxyManager/nginx-proxy-manager' },
    { id: 'portainer', title: 'Portainer', version: '2.19.4', description: 'Container management UI. Manage your containerized services through the web.', icon: '/assets/img/app-icons/portainer.webp', author: 'Portainer', dockerImage: `${R}/portainer:latest`, repoUrl: 'https://github.com/portainer/portainer' },
    { id: 'uptime-kuma', title: 'Uptime Kuma', version: '1.23.0', description: 'Self-hosted uptime monitoring. Track HTTP, TCP, DNS, and more.', icon: '/assets/img/app-icons/uptime-kuma.webp', author: 'Uptime Kuma', dockerImage: `${R}/uptime-kuma:1`, repoUrl: 'https://github.com/louislam/uptime-kuma' },
    { id: 'tailscale', title: 'Tailscale', version: '1.78.0', description: 'Zero-config VPN. Secure remote access with WireGuard mesh networking.', icon: '/assets/img/app-icons/tailscale.webp', author: 'Tailscale', dockerImage: `${R}/tailscale:stable`, repoUrl: 'https://github.com/tailscale/tailscale' },
    { id: 'netbird', title: 'NetBird', version: '0.71.2', description: 'Self-hosted WireGuard mesh VPN control plane with dashboard, embedded identity provider, management API, signal, relay, and STUN.', icon: '/assets/img/app-icons/netbird.svg', author: 'NetBird', dockerImage: 'docker.io/netbirdio/dashboard:v2.38.0', repoUrl: 'https://github.com/netbirdio/netbird' },
    { id: 'electrumx', title: 'ElectrumX', version: '1.18.0', description: 'Electrum protocol server. Index the blockchain for fast wallet lookups, privately.', icon: '/assets/img/app-icons/electrumx.png', author: 'Luke Childs', dockerImage: `${R}/electrumx:v1.18.0`, repoUrl: 'https://github.com/spesmilo/electrumx' },
    { id: 'fedimint', title: 'Fedimint Guardian', version: '0.10.0', description: 'Federated Bitcoin mint. Private, scalable Bitcoin through federated guardians.', icon: '/assets/img/app-icons/fedimint.png', author: 'Fedimint', dockerImage: `${R}/fedimintd:v0.10.0`, repoUrl: 'https://github.com/fedimint/fedimint' },
    { id: 'fedimint-clientd', title: 'Fedimint Client', version: '0.8.0', description: 'Fedimint ecash client daemon (fmcd). Lets the node hold Fedimint ecash and join federations; the wallet talks to it over a local REST API.', icon: '/assets/img/app-icons/fedimint.png', author: 'Fedimint', dockerImage: `${R}/fmcd:0.8.1`, repoUrl: 'https://github.com/minmoto/fmcd' },
    { id: 'indeedhub', title: 'Indeehub', version: '1.0.0', description: 'Bitcoin documentary streaming with Nostr identity. Stream sovereignty content.', icon: '/assets/img/app-icons/indeedhub.png', author: 'Indeehub Team', dockerImage: `${R}/indeedhub:1.0.0`, repoUrl: 'https://github.com/indeedhub/indeedhub' },
    { id: 'nostrudel', title: 'noStrudel', version: '0.40.0', category: 'nostr', description: 'Feature-rich Nostr web client. Browse feeds, post notes, manage relays with NIP-07.', icon: '/assets/img/app-icons/nostrudel.svg', author: 'hzrd149', dockerImage: '', repoUrl: 'https://github.com/hzrd149/nostrudel', webUrl: 'https://nostrudel.ninja' },
    { id: 'botfights', title: 'BotFights', version: '1.0.0', category: 'community', description: 'Bot arena + 2-player arcade fighter with controller support. AI bots battle in trivia, humans duke it out with controllers.', icon: '/assets/img/app-icons/botfights.svg', author: 'BotFights', dockerImage: `${R}/botfights:1.1.0`, repoUrl: 'https://botfights.net' },
    { id: 'gitea', title: 'Gitea', version: '1.23', category: 'development', description: 'Self-hosted Git service with container registry, CI/CD, issue tracking, and package hosting.', icon: '/assets/img/app-icons/gitea.svg', author: 'Gitea', dockerImage: 'docker.io/gitea/gitea:1.23', repoUrl: 'https://gitea.com' },
  ]
}

// Only PRIMARY containers trigger "installed" status.
// Supporting containers (DBs, caches, workers) do NOT — having only a DB
// without the main app should not mark the app as installed in the UI.
export const INSTALLED_ALIASES: Record<string, string[]> = {
  mempool: ['mempool', 'mempool-web', 'archy-mempool-web'],
  bitcoin: ['bitcoin-knots'],
  btcpay: ['btcpay-server'],
  immich: ['immich-server', 'immich-app', 'immich_server'],
  nextcloud: ['nextcloud-aio', 'nextcloud-server'],
  fedimint: ['fedimint-gateway'],
  'fedimint-clientd': ['fedimint-clientd'],
  electrumx: ['electrumx'],
  grafana: ['grafana'],
  jellyfin: ['jellyfin'],
  vaultwarden: ['vaultwarden'],
  searxng: ['searxng'],
  homeassistant: ['homeassistant'],
  photoprism: ['photoprism'],
  lnd: ['lnd'],
  filebrowser: ['filebrowser'],
  tailscale: ['tailscale'],
  netbird: ['netbird'],
  ollama: ['ollama'],
  indeedhub: ['indeedhub'],
  botfights: ['botfights'],
}

// Featured apps shown at the top of the App Store.
// The first entry with a `banner` is displayed as a full-width hero banner.
// To change the featured app, move the desired entry to position 0 and set its `banner`.
export const FEATURED_DEFINITIONS: {
  id: string
  desc: string
  tag: string
  banner?: string   // path to banner image (shown as full-width hero)
}[] = [
  {
    id: 'indeedhub',
    desc: 'Bitcoin documentaries with Nostr identity. God Bless Bitcoin, The Bitcoin Psyop, and more — streaming from your own node. No accounts, no subscriptions. Sign in with Nostr.',
    tag: 'NOSTR IDENTITY // YOUR NODE',
    banner: '/assets/img/featured/indeedhub-banner.jpg',
  },
  {
    id: 'bitcoin-knots',
    desc: 'The foundation of sovereignty. Run a full Bitcoin node to validate every transaction yourself. No trusted third parties. No asking permission. Your node enforces the consensus rules that protect your wealth. Don\'t trust — verify.',
    tag: 'FULL VALIDATION // ZERO TRUST',
  },
  {
    id: 'bitcoin-core',
    desc: 'The reference Bitcoin implementation. Same full-node guarantees as Knots, tracking upstream releases from the Bitcoin Core maintainers. Pick this if you\'d rather run mainline Bitcoin Core than Knots — both validate every block themselves.',
    tag: 'REFERENCE CLIENT // ZERO TRUST',
  },
  {
    id: 'lnd',
    desc: 'Lightning-fast payments over the Lightning Network. Open channels, route transactions, and earn routing fees — all from your sovereign node. Instant settlement. Near-zero fees. The future of money, running on your hardware.',
    tag: 'INSTANT SETTLEMENT // YOUR CHANNELS',
  },
  {
    id: 'btcpay-server',
    desc: 'Accept Bitcoin payments without intermediaries. No fees to payment processors. No KYC. No permission needed. Your commerce, your terms. Self-hosted payment infrastructure that makes you truly independent.',
    tag: 'NO INTERMEDIARIES // NO KYC',
  },
  {
    id: 'vaultwarden',
    desc: 'Your passwords belong to you. Self-hosted password vault with full Bitwarden compatibility. Zero-knowledge encryption means even you can\'t see your passwords without your master key. No cloud required — your secrets, your server.',
    tag: 'ZERO KNOWLEDGE // SELF-HOSTED',
  },
]

export function categorizeCommunityApp(app: MarketplaceApp): string {
  if (app.category) return app.category
  const id = app.id.toLowerCase()
  const title = app.title?.toLowerCase() || ''
  const description = (typeof app.description === 'string' ? app.description : app.description?.short ?? '').toLowerCase()
  const combined = `${id} ${title} ${description}`

  if (id.includes('bitcoin') || id.includes('btc') || id.includes('lightning') || id.includes('lnd') || id.includes('electr') || id.includes('fedimint') || id.includes('cashu') || combined.includes('wallet')) return 'money'
  if (id.includes('btcpay') || id.includes('commerce') || id.includes('shop') || id.includes('pos') || combined.includes('merchant')) return 'commerce'
  if (id.includes('cloud') || id.includes('nextcloud') || id.includes('storage') || id.includes('file') || id.includes('photo') || id.includes('immich') || id.includes('jellyfin') || id.includes('media') || id.includes('vault') || combined.includes('password manager')) return 'data'
  if (id.includes('home-assistant') || id.includes('homeassistant') || combined.includes('home automation')) return 'home'
  if (id.includes('nostr') || combined.includes('nostr relay')) return 'nostr'
  if (id.includes('vpn') || id.includes('wireguard') || id.includes('tailscale') || id.includes('netbird') || id.includes('proxy') || id.includes('dns') || id.includes('tor') || combined.includes('network')) return 'networking'
  if (id.includes('matrix') || id.includes('mastodon') || id.includes('chat') || id.includes('social') || combined.includes('messaging')) return 'community'
  return 'other'
}
