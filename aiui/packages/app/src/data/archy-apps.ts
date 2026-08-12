export interface ArchyAppMeta {
  id: string
  name: string
  description: string
  icon: string
  category: 'bitcoin' | 'lightning' | 'storage' | 'social' | 'tools' | 'monitoring' | 'ai' | 'identity'
  defaultPort?: number
  deepLink: string
}

export const ARCHY_APPS: ArchyAppMeta[] = [
  // ─── Bitcoin & Lightning ────────────────────────────────────────
  {
    id: 'bitcoin-core',
    name: 'Bitcoin Core',
    description: 'Full Bitcoin node — blockchain validation, UTXO set, fee estimates',
    icon: '₿',
    category: 'bitcoin',
    defaultPort: 8332,
    deepLink: '/app/bitcoin-core',
  },
  {
    id: 'lnd',
    name: 'LND',
    description: 'Lightning Network Daemon — channels, payments, invoices',
    icon: '⚡',
    category: 'lightning',
    defaultPort: 8080,
    deepLink: '/app/lnd',
  },
  {
    id: 'core-lightning',
    name: 'Core Lightning',
    description: 'C-Lightning implementation with plugin ecosystem',
    icon: '⚡',
    category: 'lightning',
    defaultPort: 9735,
    deepLink: '/app/core-lightning',
  },
  {
    id: 'btcpay-server',
    name: 'BTCPay Server',
    description: 'Self-hosted payment processor for Bitcoin and Lightning',
    icon: '🛒',
    category: 'bitcoin',
    defaultPort: 23001,
    deepLink: '/app/btcpay-server',
  },
  {
    id: 'mempool',
    name: 'Mempool',
    description: 'Blockchain explorer — visualize transactions, fees, blocks',
    icon: '🔍',
    category: 'bitcoin',
    defaultPort: 3006,
    deepLink: '/app/mempool',
  },
  {
    id: 'fedimint',
    name: 'Fedimint',
    description: 'Federated Chaumian e-cash — community custody',
    icon: '🏦',
    category: 'bitcoin',
    deepLink: '/app/fedimint',
  },
  // ─── Storage & Files ────────────────────────────────────────────
  {
    id: 'nextcloud',
    name: 'Nextcloud',
    description: 'Files, notes, contacts, calendar — self-hosted cloud',
    icon: '☁️',
    category: 'storage',
    defaultPort: 8443,
    deepLink: '/app/nextcloud',
  },
  {
    id: 'immich',
    name: 'Immich',
    description: 'Photo & video management with ML tagging',
    icon: '📸',
    category: 'storage',
    defaultPort: 2283,
    deepLink: '/app/immich',
  },
  // ─── Social & Nostr ─────────────────────────────────────────────
  {
    id: 'nostr-rs-relay',
    name: 'nostr-rs-relay',
    description: 'High-performance Nostr relay in Rust',
    icon: '🟣',
    category: 'social',
    defaultPort: 7000,
    deepLink: '/app/nostr-rs-relay',
  },
  {
    id: 'strfry',
    name: 'strfry',
    description: 'C++ Nostr relay — fast event processing',
    icon: '🟣',
    category: 'social',
    deepLink: '/app/strfry',
  },
  // ─── Tools ──────────────────────────────────────────────────────
  {
    id: 'home-assistant',
    name: 'Home Assistant',
    description: 'Smart home automation and control',
    icon: '🏠',
    category: 'tools',
    defaultPort: 8123,
    deepLink: '/app/home-assistant',
  },
  {
    id: 'searxng',
    name: 'SearXNG',
    description: 'Privacy-respecting metasearch engine',
    icon: '🔎',
    category: 'tools',
    defaultPort: 8888,
    deepLink: '/app/searxng',
  },
  {
    id: 'penpot',
    name: 'Penpot',
    description: 'Open-source design tool — prototyping and collaboration',
    icon: '🎨',
    category: 'tools',
    deepLink: '/app/penpot',
  },
  {
    id: 'onlyoffice',
    name: 'OnlyOffice',
    description: 'Document editing — docs, spreadsheets, presentations',
    icon: '📝',
    category: 'tools',
    deepLink: '/app/onlyoffice',
  },
  {
    id: 'meshtastic',
    name: 'Meshtastic',
    description: 'Off-grid mesh networking over LoRa radios',
    icon: '📡',
    category: 'tools',
    deepLink: '/app/meshtastic',
  },
  // ─── Monitoring ─────────────────────────────────────────────────
  {
    id: 'grafana',
    name: 'Grafana',
    description: 'Monitoring dashboards — system metrics and alerts',
    icon: '📊',
    category: 'monitoring',
    defaultPort: 3000,
    deepLink: '/app/grafana',
  },
  // ─── AI ─────────────────────────────────────────────────────────
  {
    id: 'ollama',
    name: 'Ollama',
    description: 'Run LLMs locally — Llama, Mistral, Gemma',
    icon: '🧠',
    category: 'ai',
    defaultPort: 11434,
    deepLink: '/app/ollama',
  },
  // ─── Identity ───────────────────────────────────────────────────
  {
    id: 'did-wallet',
    name: 'DID Wallet',
    description: 'Decentralized identity — Web5 verifiable credentials',
    icon: '🪪',
    category: 'identity',
    deepLink: '/app/did-wallet',
  },
]

/** Look up an Archy app by its ID */
export function getArchyApp(id: string): ArchyAppMeta | undefined {
  return ARCHY_APPS.find((a) => a.id === id)
}
