/**
 * Marketplace data: curated app list, categorization, aliases, and tier logic.
 * Extracted from Marketplace.vue to keep the view under 500 lines.
 */

export interface MarketplaceApp {
  id: string
  title?: string
  version?: string
  description?: string | { short?: string; long?: string }
  icon?: string
  author?: string
  dockerImage?: string
  manifestUrl?: string
  repoUrl?: string
  webUrl?: string
  screenshots?: AppScreenshot[]
  category?: string
  source?: string
  url?: string
  s9pkUrl?: string
  containerConfig?: {
    ports?: string[]
    volumes?: string[]
    env?: string[]
    command?: string
    args?: string[]
  }
  trustScore?: number
  trustTier?: string
  relayCount?: number
  /**
   * DID-signature verdict for relay-discovered apps. `undefined` for curated
   * and local apps, which don't travel through the marketplace protocol at all.
   */
  signature?: AppSignature
  /** The `author.did` the signature was checked against, for the tooltip. */
  authorDid?: string
}

export type AppSignature =
  | { status: 'valid' }
  | { status: 'missing' }
  | { status: 'invalid'; reason: string }

export type AppScreenshot = string | {
  src: string
  alt?: string
}

export interface InstallProgress {
  id: string
  title: string
  status: 'downloading' | 'installing' | 'starting' | 'complete' | 'error'
  progress: number
  message: string
  attempt: number
}

/** Apps that ask for their concrete version in InstallVersionModal. Their
 * store tiles deliberately omit a single catalog version: showing “v28.4”
 * there implies that is the only version immediately before asking the user
 * to choose a different one. */
export const MULTI_VERSION_APP_IDS = new Set(['bitcoin-knots', 'bitcoin-core'])

/** Archipelago app registry — all app images are mirrored here */
const REGISTRY = 'source.archipelago-foundation.org/lfg2025'

/** Marketplace app ID -> backend package keys (for "Already Installed" when first-boot/deploy created them) */
export const INSTALLED_ALIASES: Record<string, string[]> = {
  mempool: ['mempool-web', 'mempool-api', 'archy-mempool-web', 'archy-mempool-db'],
  bitcoin: ['bitcoin-knots'],
  btcpay: ['btcpay-server', 'archy-btcpay-db', 'archy-nbxplorer'],
  immich: ['immich-server', 'immich-app', 'immich_server', 'immich_postgres', 'immich_redis'],
  nextcloud: ['nextcloud-aio', 'nextcloud-server'],
  fedimint: ['fedimint-gateway'],
  electrumx: ['electrumx', 'archy-electrs-ui'],
  grafana: ['grafana'],
  jellyfin: ['jellyfin'],
  vaultwarden: ['vaultwarden'],
  searxng: ['searxng'],
  homeassistant: ['homeassistant'],
  photoprism: ['photoprism'],
  lnd: ['lnd', 'archy-lnd-ui'],
  filebrowser: ['filebrowser'],
  tailscale: ['tailscale'],
  netbird: ['netbird'],
  ollama: ['ollama'],
}

/** Get app tier classification (matches backend get_app_tier) */
export function getAppTier(appId: string): string {
  const core = ['bitcoin-knots', 'bitcoin', 'lnd', 'mempool', 'btcpay-server', 'filebrowser']
  const recommended = ['fedimint', 'vaultwarden', 'uptime-kuma', 'grafana', 'searxng', 'tailscale', 'netbird', 'portainer']
  if (core.includes(appId)) return 'core'
  if (recommended.includes(appId)) return 'recommended'
  return 'optional'
}

/** Categorize community apps based on their ID and description */
export function categorizeCommunityApp(app: MarketplaceApp): string {
  if (app.category) return app.category

  const id = app.id.toLowerCase()
  const title = app.title?.toLowerCase() || ''
  const description = (typeof app.description === 'string' ? app.description : (app.description as { short?: string })?.short ?? '').toLowerCase()
  const combined = `${id} ${title} ${description}`

  if (id.includes('bitcoin') || id.includes('btc') || id.includes('lightning') ||
      id.includes('lnd') || id.includes('cln') || id.includes('electr') ||
      id.includes('fedimint') || id.includes('cashu') || title.includes('lightning') ||
      combined.includes('wallet') || combined.includes('satoshi')) {
    return 'money'
  }

  if (id.includes('btcpay') || id.includes('commerce') || id.includes('shop') ||
      id.includes('store') || id.includes('pos') || id.includes('payment') ||
      combined.includes('merchant') || combined.includes('invoice')) {
    return 'commerce'
  }

  if (id.includes('cloud') || id.includes('nextcloud') || id.includes('sync') ||
      id.includes('storage') || id.includes('backup') || id.includes('file') ||
      id.includes('photo') || id.includes('immich') || id.includes('jellyfin') ||
      id.includes('plex') || id.includes('media') || id.includes('vault') ||
      combined.includes('password manager') || combined.includes('file storage')) {
    return 'data'
  }

  if (id.includes('home-assistant') || id.includes('homeassistant') ||
      id.includes('smart-home') || id.includes('automation') || id.includes('iot') ||
      combined.includes('home automation') || combined.includes('smart home')) {
    return 'home'
  }

  if (id.includes('nostr') || (id.includes('relay') && combined.includes('nostr')) ||
      combined.includes('nostr relay') || combined.includes('nostr client')) {
    return 'nostr'
  }

  if (id.includes('vpn') || id.includes('wireguard') || id.includes('tailscale') || id.includes('netbird') ||
      id.includes('proxy') || id.includes('dns') || id.includes('pihole') ||
      id.includes('adguard') || id.includes('nginx') || id.includes('tor') ||
      combined.includes('network') || combined.includes('firewall')) {
    return 'networking'
  }

  if (id.includes('matrix') || id.includes('synapse') || id.includes('element') ||
      id.includes('mastodon') || id.includes('lemmy') ||
      id.includes('messenger') || id.includes('chat') || id.includes('social') ||
      id.includes('cups') || combined.includes('communication') ||
      combined.includes('messaging')) {
    return 'community'
  }

  return 'other'
}

/** Curated list of apps with Docker Hub images */
export function getCuratedAppList(): MarketplaceApp[] {
  return [
    {
      id: 'bitcoin-knots',
      title: 'Bitcoin Knots',
      version: '28.1.0',
      description: 'Run a full Bitcoin node. Validate and relay blocks and transactions on the Bitcoin network.',
      icon: '/assets/img/app-icons/bitcoin-knots.webp',
      author: 'Bitcoin Knots',
      dockerImage: `${REGISTRY}/bitcoin-knots:latest`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/bitcoinknots/bitcoin'
    },
    {
      id: 'btcpay-server',
      title: 'BTCPay Server',
      version: '2.4.2',
      description: 'Self-hosted Bitcoin payment processor. Accept Bitcoin payments without intermediaries or fees.',
      icon: '/assets/img/app-icons/btcpay-server.png',
      author: 'BTCPay Server Foundation',
      dockerImage: 'docker.io/btcpayserver/btcpayserver:2.4.2',
      manifestUrl: undefined,
      repoUrl: 'https://github.com/btcpayserver/btcpayserver'
    },
    {
      id: 'lnd',
      title: 'LND',
      version: '0.18.4',
      description: 'Lightning Network Daemon. Fast and cheap Bitcoin payments through the Lightning Network.',
      icon: '/assets/img/app-icons/lnd.png',
      author: 'Lightning Labs',
      dockerImage: `${REGISTRY}/lnd:v0.18.4-beta`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/lightningnetwork/lnd'
    },
    {
      id: 'mempool',
      title: 'Mempool Explorer',
      version: '3.0.0',
      description: 'Self-hosted Bitcoin blockchain and mempool visualizer with beautiful explorer interface.',
      icon: '/assets/img/app-icons/mempool.webp',
      author: 'Mempool',
      dockerImage: `${REGISTRY}/mempool-frontend:v3.0.0`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/mempool/mempool'
    },
    {
      id: 'homeassistant',
      title: 'Home Assistant',
      version: '2024.1',
      description: 'Open-source home automation platform. Control and automate your smart home devices privately.',
      icon: '/assets/img/app-icons/homeassistant.png',
      author: 'Home Assistant',
      dockerImage: `${REGISTRY}/home-assistant:2024.1`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/home-assistant/core'
    },
    {
      id: 'grafana',
      title: 'Grafana',
      version: '10.2.0',
      description: 'Analytics and monitoring platform. Create dashboards and visualize data from multiple sources.',
      icon: '/assets/img/app-icons/grafana.png',
      author: 'Grafana Labs',
      dockerImage: `${REGISTRY}/grafana:10.2.0`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/grafana/grafana'
    },
    {
      id: 'searxng',
      title: 'SearXNG',
      version: '2024.1.0',
      description: 'Privacy-respecting metasearch engine. Search without tracking or ads.',
      icon: '/assets/img/app-icons/searxng.png',
      author: 'SearXNG',
      dockerImage: `${REGISTRY}/searxng:latest`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/searxng/searxng'
    },
    {
      id: 'ollama',
      title: 'Ollama',
      version: '0.1.0',
      description: 'Run large language models locally. Download and run AI models like Llama, Mistral on your own hardware.',
      icon: '/assets/img/app-icons/ollama.png',
      author: 'Ollama',
      dockerImage: `${REGISTRY}/ollama:latest`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/ollama/ollama'
    },
    {
      id: 'cryptpad',
      title: 'CryptPad',
      version: '2024.12.0',
      description: 'End-to-end encrypted documents, spreadsheets, and presentations. Zero-knowledge collaboration.',
      icon: '/assets/icon/favico-black-v2.svg',
      author: 'XWiki SAS',
      dockerImage: `${REGISTRY}/cryptpad:2024.12.0`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/cryptpad/cryptpad'
    },
    {
      id: 'nextcloud',
      title: 'Nextcloud',
      version: '29.0',
      description: 'Self-hosted cloud storage and collaboration platform. Your own private cloud.',
      icon: '/assets/img/app-icons/nextcloud.webp',
      author: 'Nextcloud',
      dockerImage: `${REGISTRY}/nextcloud:29`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/nextcloud/server'
    },
    {
      id: 'vaultwarden',
      title: 'Vaultwarden',
      version: '1.30.0',
      description: 'Self-hosted password manager (Bitwarden-compatible). Secure vault for passwords and secrets.',
      icon: '/assets/img/app-icons/vaultwarden.webp',
      author: 'Vaultwarden',
      dockerImage: `${REGISTRY}/vaultwarden:1.30.0-alpine`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/dani-garcia/vaultwarden'
    },
    {
      id: 'jellyfin',
      title: 'Jellyfin',
      version: '10.8.0',
      description: 'Free media server system. Stream your movies, music, and photos to any device.',
      icon: '/assets/img/app-icons/jellyfin.webp',
      author: 'Jellyfin',
      dockerImage: `${REGISTRY}/jellyfin:10.8.13`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/jellyfin/jellyfin'
    },
    {
      id: 'photoprism',
      title: 'PhotoPrism',
      version: '240915',
      description: 'AI-powered photo management. Organize and browse photos with facial recognition.',
      icon: '/assets/img/app-icons/photoprism.svg',
      author: 'PhotoPrism',
      dockerImage: `${REGISTRY}/photoprism:240915`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/photoprism/photoprism'
    },
    {
      id: 'immich',
      title: 'Immich',
      version: '1.90.0',
      description: 'High-performance self-hosted photo and video backup. Mobile-first with ML features.',
      icon: '/assets/img/app-icons/immich.png',
      author: 'Immich',
      dockerImage: `${REGISTRY}/immich:release`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/immich-app/immich'
    },
    {
      id: 'electrumx',
      title: 'ElectrumX',
      version: '1.18.0',
      description: 'Electrum protocol server. Index the blockchain for fast wallet lookups, privately.',
      icon: '/assets/img/app-icons/electrumx.png',
      author: 'Luke Childs',
      dockerImage: `${REGISTRY}/electrumx:v1.18.0`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/spesmilo/electrumx'
    },
    {
      id: 'filebrowser',
      title: 'File Browser',
      version: '2.27.0',
      description: 'Web-based file manager. Browse, upload, and manage files through a web interface.',
      icon: '/assets/img/app-icons/file-browser.webp',
      author: 'File Browser',
      dockerImage: `${REGISTRY}/filebrowser:v2.27.0`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/filebrowser/filebrowser'
    },
    {
      id: 'nginx-proxy-manager',
      title: 'Nginx Proxy Manager',
      version: '2.11.0',
      description: 'Easy proxy management with SSL. Beautiful web interface for managing reverse proxies.',
      icon: '/assets/img/app-icons/nginx.svg',
      author: 'Nginx Proxy Manager',
      dockerImage: `${REGISTRY}/nginx-proxy-manager:latest`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/NginxProxyManager/nginx-proxy-manager'
    },
    {
      id: 'portainer',
      title: 'Portainer',
      version: '2.19.0',
      description: 'Container management UI. Manage Docker containers through a beautiful web interface.',
      icon: '/assets/img/app-icons/portainer.webp',
      author: 'Portainer',
      dockerImage: `${REGISTRY}/portainer:latest`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/portainer/portainer'
    },
    {
      id: 'uptime-kuma',
      title: 'Uptime Kuma',
      version: '1.23.0',
      description: 'Self-hosted monitoring tool. Monitor uptime for HTTP(s), TCP, DNS, and more.',
      icon: '/assets/img/app-icons/uptime-kuma.webp',
      author: 'Uptime Kuma',
      dockerImage: `${REGISTRY}/uptime-kuma:1`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/louislam/uptime-kuma'
    },
    {
      id: 'tailscale',
      title: 'Tailscale',
      version: '1.78.0',
      description: 'Zero-config VPN for secure remote access. Connect all your devices with WireGuard mesh network.',
      icon: '/assets/img/app-icons/tailscale.webp',
      author: 'Tailscale',
      dockerImage: `${REGISTRY}/tailscale:stable`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/tailscale/tailscale'
    },
    {
      id: 'netbird',
      title: 'NetBird',
      version: '0.71.2',
      description: 'Self-hosted WireGuard mesh VPN control plane with dashboard, embedded identity provider, management API, signal, relay, and STUN.',
      icon: '/assets/img/app-icons/netbird.svg',
      author: 'NetBird',
      dockerImage: 'docker.io/netbirdio/dashboard:v2.38.0',
      manifestUrl: undefined,
      repoUrl: 'https://github.com/netbirdio/netbird'
    },
    {
      id: 'fedimint',
      title: 'Fedimint',
      version: '0.10.0',
      description: 'Federated Bitcoin mint with built-in Guardian UI. Private, scalable Bitcoin through federated guardians.',
      icon: '/assets/img/app-icons/fedimint.png',
      author: 'Fedimint',
      dockerImage: `${REGISTRY}/fedimintd:v0.10.0`,
      manifestUrl: undefined,
      repoUrl: 'https://github.com/fedimint/fedimint'
    },
    {
      id: 'indeedhub',
      title: 'Indeehub',
      version: '0.1.0',
      description: 'Bitcoin documentary streaming platform with Nostr identity sign-in. Stream God Bless Bitcoin and other educational content about sovereignty and decentralized technology.',
      icon: '/assets/img/app-icons/indeedhub.png',
      author: 'Indeehub Team',
      dockerImage: 'source.archipelago-foundation.org/lfg2025/indeedhub:latest',
      manifestUrl: undefined,
      repoUrl: 'https://github.com/indeedhub/indeedhub'
    },
    {
      id: 'nostrudel',
      title: 'noStrudel',
      version: '0.40.0',
      category: 'nostr',
      description: 'A feature-rich Nostr web client with NIP-07 signer support. Browse your feed, post notes, manage relays, and interact with the Nostr network — all signed with your node\'s Nostr identity.',
      icon: '/assets/img/app-icons/nostrudel.svg',
      author: 'hzrd149',
      dockerImage: '',
      manifestUrl: undefined,
      repoUrl: 'https://github.com/hzrd149/nostrudel',
      webUrl: 'https://nostrudel.ninja'
    },
    {
      id: 'botfights',
      title: 'BotFights',
      version: '1.0.0',
      category: 'community',
      description: 'Bot arena + 2-player arcade fighter with controller support. AI bots battle in trivia, humans duke it out with controllers.',
      icon: '/assets/img/app-icons/botfights.svg',
      author: 'BotFights',
      dockerImage: `${REGISTRY}/botfights:1.1.0`,
      manifestUrl: undefined,
      repoUrl: 'https://botfights.net',
    },
    {
      id: 'gitea',
      title: 'Gitea',
      version: '1.23',
      category: 'data',
      description: 'Self-hosted Git service with built-in container registry, CI/CD, issue tracking, and package hosting. Lightweight alternative to GitHub/GitLab.',
      icon: '/assets/img/app-icons/gitea.svg',
      author: 'Gitea',
      dockerImage: 'docker.io/gitea/gitea:1.23',
      manifestUrl: undefined,
      repoUrl: 'https://gitea.com',
    },
  ]
}
