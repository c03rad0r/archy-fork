/**
 * Mock Archy node data for standalone dev/testing.
 * Provides realistic data for all 6 context categories so you can test
 * the Archy integration without running inside an actual Archipelago node.
 *
 * Enable with: VITE_MOCK_ARCHY=true or ?mockArchy URL param
 */

import type { ArchyWalletInfo, ArchyBitcoinInfo, ArchyFileEntry } from '@/composables/useArchy'

export interface MockArchyApp {
  id: string
  name: string
  state: 'running' | 'stopped'
  status: string
}

export const mockArchyApps: MockArchyApp[] = [
  { id: 'bitcoin-core', name: 'Bitcoin Core', state: 'running', status: 'Synced — Block 893,412' },
  { id: 'lnd', name: 'LND', state: 'running', status: '6 channels, 12 peers' },
  { id: 'mempool', name: 'Mempool', state: 'running', status: 'Healthy' },
  { id: 'btcpay-server', name: 'BTCPay Server', state: 'running', status: 'Healthy' },
  { id: 'nextcloud', name: 'Nextcloud', state: 'running', status: '847 files, 12.4 GB used' },
  { id: 'immich', name: 'Immich', state: 'running', status: '2,341 photos, 89 videos' },
  { id: 'nostr-rs-relay', name: 'nostr-rs-relay', state: 'running', status: '14,203 events' },
  { id: 'home-assistant', name: 'Home Assistant', state: 'stopped', status: 'Stopped' },
  { id: 'searxng', name: 'SearXNG', state: 'running', status: 'Healthy' },
  { id: 'grafana', name: 'Grafana', state: 'running', status: 'Healthy' },
  { id: 'ollama', name: 'Ollama', state: 'stopped', status: 'Stopped' },
]

export const mockArchySystem = {
  name: 'Archipelago',
  version: '0.9.2',
}

export const mockArchyNetwork = {
  connected: true,
}

export const mockArchyWallet: ArchyWalletInfo = {
  available: true,
  status: 'active',
  alias: 'MyNode',
  num_active_channels: 6,
  num_peers: 12,
  synced_to_chain: true,
  block_height: 893412,
  balance_sats: 1_250_000,
  channel_balance_sats: 3_500_000,
  pending_open_balance: 0,
}

export const mockArchyBitcoin: ArchyBitcoinInfo = {
  available: true,
  block_height: 893412,
  sync_progress: 1.0,
  chain: 'mainnet',
  mempool_tx_count: 42_350,
  mempool_size: 128_000_000,
}

export const mockArchyFiles: ArchyFileEntry[] = [
  { name: 'Documents', path: '/Documents', type: 'folder' },
  { name: 'Photos', path: '/Photos', type: 'folder' },
  { name: 'Music', path: '/Music', type: 'folder' },
  { name: 'Videos', path: '/Videos', type: 'folder' },
  { name: 'family-reunion-2024.jpg', path: '/Photos/family-reunion-2024.jpg', type: 'file', size: 4_200_000, modified: '2024-12-25' },
  { name: 'sunset-beach.jpg', path: '/Photos/sunset-beach.jpg', type: 'file', size: 3_800_000, modified: '2024-11-15' },
  { name: 'node-setup-guide.pdf', path: '/Documents/node-setup-guide.pdf', type: 'file', size: 2_100_000, modified: '2024-10-01' },
  { name: 'bitcoin-whitepaper.pdf', path: '/Documents/bitcoin-whitepaper.pdf', type: 'file', size: 184_000, modified: '2024-01-03' },
  { name: 'household-budget.ods', path: '/Documents/household-budget.ods', type: 'file', size: 45_000, modified: '2025-02-28' },
  { name: 'birthday-video.mp4', path: '/Videos/birthday-video.mp4', type: 'file', size: 250_000_000, modified: '2025-01-15' },
  { name: 'conference-talk.mp4', path: '/Videos/conference-talk.mp4', type: 'file', size: 180_000_000, modified: '2024-09-20' },
  { name: 'podcast-episode.mp3', path: '/Music/podcast-episode.mp3', type: 'file', size: 45_000_000, modified: '2025-03-01' },
  { name: 'backup-keys.txt', path: '/Documents/backup-keys.txt', type: 'file', size: 1200, modified: '2024-06-15' },
  { name: 'recipes.md', path: '/Documents/recipes.md', type: 'file', size: 8500, modified: '2025-02-14' },
  { name: 'travel-plans.md', path: '/Documents/travel-plans.md', type: 'file', size: 3200, modified: '2025-03-05' },
]
