/**
 * AppDetails data: URL maps, route-to-package mappings, aliases, and status helpers.
 * Extracted from AppDetails.vue to keep the view under 500 lines.
 */
import { PackageState } from '@/types/api'

/** Web-only app detection (no container -- external websites) */
export const WEB_ONLY_APP_URLS: Record<string, string> = {}

/** Map route/marketplace app IDs to backend package keys (container names). */
export const ROUTE_TO_PACKAGE_KEY: Record<string, string> = {
  mempool: 'mempool',
  'mempool-electrs': 'mempool-electrs',
  electrs: 'mempool-electrs',
  btcpay: 'btcpay-server',
  'btcpay-server': 'btcpay-server',
  fedimint: 'fedimint',
  'fedimint-gateway': 'fedimint-gateway',
  lnd: 'lnd',
  'lnd-ui': 'lnd',
  bitcoin: 'bitcoin-knots',
  'bitcoin-knots': 'bitcoin-knots',
  homeassistant: 'homeassistant',
  'home-assistant': 'homeassistant',
  grafana: 'grafana',
  searxng: 'searxng',
  ollama: 'ollama',
  nextcloud: 'nextcloud',
  vaultwarden: 'vaultwarden',
  jellyfin: 'jellyfin',
  photoprism: 'photoprism',
  immich: 'immich',
  filebrowser: 'filebrowser',
  'nginx-proxy-manager': 'nginx-proxy-manager',
  'gitea': 'gitea',
  portainer: 'portainer',
  'uptime-kuma': 'uptime-kuma',
  tailscale: 'tailscale',
  netbird: 'netbird',
}

/** Backend may register under variant container names */
export const PACKAGE_ALIASES: Record<string, string[]> = {
  immich: ['immich_server', 'immich-server'],
  nextcloud: ['nextcloud-aio', 'nextcloud-server'],
}

export function resolvePackageKey(routeId: string): string {
  return ROUTE_TO_PACKAGE_KEY[routeId] ?? routeId
}

/** Apps that depend on Bitcoin being synced */
export const BITCOIN_DEPENDENT_APPS = ['lnd', 'electrumx', 'electrs', 'mempool-electrs', 'btcpay-server', 'btcpayserver']

/** V3 onion addresses are 56+ chars + .onion. Placeholders like "btcpay.onion" are not real. */
export function isRealOnionAddress(addr: string | undefined): boolean {
  return !!(addr && addr.endsWith('.onion') && addr.length >= 60 && addr.length <= 70)
}

export function getStatusClass(state: PackageState, health?: string | null, exitCode?: number | null): string {
  if (state === PackageState.Running && health === 'starting') return 'bg-yellow-500/20 text-yellow-200 border border-yellow-500/30'
  if (state === PackageState.Running && health === 'unhealthy') return 'bg-orange-500/20 text-orange-200 border border-orange-500/30'
  switch (state) {
    case PackageState.Running:
      return 'bg-green-500/20 text-green-200 border border-green-500/30'
    case PackageState.Stopped:
      return 'bg-gray-500/20 text-gray-200 border border-gray-500/30'
    case PackageState.Exited:
      return exitCode != null && exitCode !== 0
        ? 'bg-red-500/20 text-red-200 border border-red-500/30'
        : 'bg-gray-500/20 text-gray-200 border border-gray-500/30'
    case PackageState.Starting:
    case PackageState.Stopping:
    case PackageState.Restarting:
      return 'bg-yellow-500/20 text-yellow-200 border border-yellow-500/30'
    case PackageState.Installing:
      return 'bg-blue-500/20 text-blue-200 border border-blue-500/30'
    case PackageState.Updating:
      return 'bg-orange-500/20 text-orange-200 border border-orange-500/30'
    default:
      return 'bg-gray-500/20 text-gray-200 border border-gray-500/30'
  }
}

export function getStatusDotClass(state: PackageState, health?: string | null, exitCode?: number | null): string {
  if (state === PackageState.Running && health === 'starting') return 'bg-yellow-400 animate-pulse'
  if (state === PackageState.Running && health === 'unhealthy') return 'bg-orange-400 animate-pulse'
  switch (state) {
    case PackageState.Running:
      return 'bg-green-400'
    case PackageState.Stopped:
      return 'bg-gray-400'
    case PackageState.Exited:
      return exitCode != null && exitCode !== 0
        ? 'bg-red-400 animate-pulse'
        : 'bg-gray-400'
    case PackageState.Starting:
    case PackageState.Stopping:
    case PackageState.Restarting:
      return 'bg-yellow-400 animate-pulse'
    case PackageState.Installing:
      return 'bg-blue-400 animate-pulse'
    case PackageState.Updating:
      return 'bg-orange-400 animate-pulse'
    default:
      return 'bg-gray-400'
  }
}

export function getStatusLabel(state: PackageState, health?: string | null, exitCode?: number | null): string {
  if (state === PackageState.Updating) return 'updating...'
  if (state === PackageState.Running && health === 'starting') return 'starting up'
  if (state === PackageState.Running && health === 'unhealthy') return 'unhealthy'
  if (state === PackageState.Running && health === 'healthy') return 'healthy'
  if (state === PackageState.Exited) {
    if (exitCode === 137) return 'killed (SIGKILL)'
    if (exitCode != null && exitCode !== 0) return 'crashed'
    return 'stopped'
  }
  return state
}
